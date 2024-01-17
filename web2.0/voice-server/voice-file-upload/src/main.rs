mod util;

use std::{env::var, sync::Arc};

use axum::{
	body::Bytes,
	extract::{State, DefaultBodyLimit},
	http::{
		header::{self},
		uri, HeaderMap, StatusCode, Uri,
	},
	response::IntoResponse,
	routing::post,
	Router,
};

use tokio::{net::TcpListener, process::Command, sync::OnceCell};

use voice_shared::{RemoteFileIdentifier, RemoteFileKind, RemoteFileManager};

use crate::util::BooleanOption;

#[tokio::main]
async fn main() {
	let router = Router::new()
		.route("/", post(upload_file))
		.layer(tower_http::cors::CorsLayer::permissive())
		.layer(DefaultBodyLimit::max(1024 * 1024 * 100))
		.with_state(voice_shared::debug_remote::file_manager().into());
	axum::serve(
		TcpListener::bind((
			"0.0.0.0",
			dbg!(var("PORT").map(|x| x.parse().unwrap()).unwrap_or(3002)),
		))
		.await
		.unwrap(),
		router,
	)
	.await
	.unwrap();
}

/// Upload a file
/// # Example 1
/// POST / HTTP/1.1
/// Content-Type: application/octet-stream
///
/// <file>
/// # Example 2
/// POST / HTTP/1.1
/// Content-Type: text/x-url
///
/// https://example.com/file.mp4
/// # Possible responses
/// 200, with the identifier of the file
/// 400, if the request is malformed
/// 422, if the file at the url is not accessible (error isn't detailed on purpose)
/// 500, if the file could not be written or something else goes wrong
async fn upload_file<T: RemoteFileManager>(
	State(file_manager): State<Arc<T>>,
	headers: HeaderMap,
	body: Bytes,
) -> Result<String, (StatusCode, &'static str)> {
	let Some(content_type) =
		headers.get(header::CONTENT_TYPE).and_then(|x| x.to_str().ok())
	else {
		return Err((StatusCode::BAD_REQUEST, "content-type header is required"));
	};

	let file = match content_type {
		"application/octet-stream" => body.to_vec(),
		"text/x-url" => {
			let uri: Uri = std::str::from_utf8(&body)
				.ok()
				.and_then(|x| x.parse().ok())
				.ok_or((StatusCode::BAD_REQUEST, "invalid url"))?;
			if let Some(true) =
				uri.scheme().map(|x| x == &uri::Scheme::HTTP || x == &uri::Scheme::HTTPS)
			{
				load_file_from_url(uri).await.map_err(|_| {
					(StatusCode::UNPROCESSABLE_ENTITY, "error when reaching url")
				})?
			} else {
				return Err((
					StatusCode::BAD_REQUEST,
					"scheme should be 'http' or 'https'",
				));
			}
		}
		_ => return Err((
			StatusCode::BAD_REQUEST,
			"content type should be either 'application/octet-stream' or 'text/x-url'",
		)),
	};

	// file should be valid at this point

	let remote_file = file_manager
		.upload_file(&file, RemoteFileKind::VideoInput)
		.await
		.map_err(|x| match x {
		voice_shared::RemoteFileManagerError::WriteError => {
			(StatusCode::INTERNAL_SERVER_ERROR, "failed to write file")
		}
		voice_shared::RemoteFileManagerError::ReadError => {
			(StatusCode::INTERNAL_SERVER_ERROR, "failed to read file")
		}
		voice_shared::RemoteFileManagerError::Unspecified(x) => {
			println!("unspecified error: {:?}", x);
			(StatusCode::INTERNAL_SERVER_ERROR, "unknown error")
		}
		voice_shared::RemoteFileManagerError::ChildError(x) => {
			println!("child process error: {:?}", x);
			(StatusCode::INTERNAL_SERVER_ERROR, "unknown error")
		}
	})?;

	println!("uploaded file: {:?}", remote_file);

	Ok(remote_file.identifier().to_string())
}

/// Helper function to load a file from a url
async fn load_file_from_url(url: Uri) -> Result<Vec<u8>, StatusCode> {
	// TODO: move somewhere else
	const MAX_FILE_SIZE: u64 = 1024 * 1024 * 100; // 100 MiB

	// reqwest client singleton
	static REQWEST_CLIENT: tokio::sync::OnceCell<reqwest::Client> = OnceCell::const_new();

	let client = REQWEST_CLIENT
		.get_or_try_init(|| async {
			let mut headers = reqwest::header::HeaderMap::new();
			headers.insert(
				reqwest::header::USER_AGENT,
				"voice-file-upload".parse().unwrap(),
			);
			let client = reqwest::Client::builder().default_headers(headers).build();
			client.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
		})
		.await?;

	let url_string = url.to_string();

	// first check the content length with a HEAD request
	// endpoint must respond with a 'Content-Length' or we bail

	// this expression returns if the above doesn't hold
	client
		.head(&url_string)
		.send()
		.await
		.map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?
		.content_length()
		.and_then(|l| (l <= MAX_FILE_SIZE).option())
		.ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;

	// now we can download the file
	// reqwest uses `hyper` for http, which, according to https://github.com/seanmonstar/warp/issues/326
	// limits the body size to Content-Length, so we don't need to check the bytes read.
	let res = client
		.get(&url_string)
		.send()
		.await
		.map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
	if !res.status().is_success() {
		return Err(StatusCode::UNPROCESSABLE_ENTITY);
	}
	let body = res.bytes().await.map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
	Ok(body.into())
}
