mod util;

use std::{borrow::Cow, env::var, ops::Deref, sync::Arc, time::Duration};

use axum::{
	body::Bytes,
	extract::{DefaultBodyLimit, Path, Query, State},
	http::{
		header::{self},
		uri, HeaderMap, HeaderName, HeaderValue, StatusCode, Uri,
	},
	response::IntoResponse,
	routing::{get, post, put},
	Json, Router,
};

use serde::Deserialize;
use tap::{Pipe, Tap};
use tokio::{net::TcpListener, process::Command, sync::OnceCell};

use voice_shared::{RemoteFileIdentifier, RemoteFileKind, RemoteFileManager};

use crate::util::BooleanOption;

struct ReqwestSingleton(tokio::sync::OnceCell<reqwest::Client>);

impl ReqwestSingleton {
	const fn new() -> Self {
		Self(tokio::sync::OnceCell::const_new())
	}

	async fn get(&self) -> reqwest::Client {
		REQWEST_CLIENT
			.0
			.get_or_try_init(|| async {
				let mut headers = reqwest::header::HeaderMap::new();
				headers.insert(
					reqwest::header::USER_AGENT,
					"voice-file-upload".parse().unwrap(),
				);
				let client = reqwest::Client::builder()
					.connect_timeout(REQWEST_CONNECT_TIMEOUT)
					.default_headers(headers)
					.build();
				client.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
			})
			.await
			.expect("failed to initialize reqwest client")
			// reqwest::Client is an Arc internally
			.clone()
	}
}

// reqwest client singleton
static REQWEST_CLIENT: ReqwestSingleton = ReqwestSingleton::new();

// the server is not worth our time if it's a slowpoke
const REQWEST_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

const MAX_FREE_FILE_SIZE: usize = 1024 * 1024 * 100; // 100MB
const MAX_PREMIUM_FILE_SIZE: usize = 1024 * 1024 * 1000; // 1GB

fn check_file_size(file_size: usize, is_premium: bool) -> bool {
	if is_premium {
		file_size <= MAX_PREMIUM_FILE_SIZE
	} else {
		file_size <= MAX_FREE_FILE_SIZE
	}
}

#[tokio::main]
async fn main() {
	voice_shared::axum_serve(
		Router::new()
			.route("/check-upload-url", put(check_upload_url))
			.route("/constants", get(get_constants))
			.route("/upload-file", post(upload_file))
			.route("/read-file/:file_id", get(read_file))
			.layer(tower_http::cors::CorsLayer::permissive())
			.layer(DefaultBodyLimit::max(MAX_PREMIUM_FILE_SIZE))
			.layer(tower_http::compression::CompressionLayer::new().br(true))
			.with_state(voice_shared::debug_remote::file_manager().into()),
		3002,
	)
	.await;
}

#[derive(Deserialize)]
struct IsPremiumQuery {
	premium: bool,
}

async fn get_constants(
	Query(IsPremiumQuery { premium }): Query<IsPremiumQuery>,
) -> ([(HeaderName, &'static str); 1], Json<serde_json::Value>) {
	// let mut headers = HeaderMap::new();
	// headers.insert("Cache-Control", "no-cache".parse().unwrap());
	const SILENCE_CUTOFF: (i32, i32) = (-90, -10);
	const SKIP_DURATION: (i32, i32) = (100, 250);

	let json = serde_json::json!({
		"silenceCutoff": {
			"min": SILENCE_CUTOFF.0,
			"max": SILENCE_CUTOFF.1,
		},
		"skipDuration": {
			"min": SKIP_DURATION.0,
			"max": SKIP_DURATION.1,
		},
		"maxFileSize": if premium { MAX_PREMIUM_FILE_SIZE } else { MAX_FREE_FILE_SIZE },
	});

	([(header::CACHE_CONTROL, "no-cache")], Json(json))
}

/// Check if the upload url really contains a video file, and if it's not too big
///
/// # Example 1
/// ```
/// PUT /check_upload_url?is_premium=<bool>
///
/// https://example.com/file.mp4
/// ```
/// Responds with 200 if the url is valid, 422 if it's not
async fn check_upload_url(
	Query(IsPremiumQuery { premium }): Query<IsPremiumQuery>,
	body: String,
) -> Result<(), (StatusCode, String)> {
	check_upload_url_impl(&body, premium)
		.await
		.map_err(|x| (x.status_code(), x.message().to_owned()))
		.tap(|x| println!("check upload url ({}) result: {:?}", body, x))
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
	println!(
		"uploading_file route {}, x-premium: {}",
		body.len(),
		headers.get("x-premium").is_some()
	);
	let Some(content_type) =
		headers.get(header::CONTENT_TYPE).and_then(|x| x.to_str().ok())
	else {
		return Err((StatusCode::BAD_REQUEST, "content-type header is required"));
	};

	// TODO: replace with actual authenfication when it's ready
	let is_premium = headers.get("x-premium").is_some();

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
				load_file_from_url(uri, is_premium).await.map_err(|_| {
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

enum CheckUploadUrlError {
	BadUrl,
	Unreachable,
	RequestError,
	NotVideo,
	NoContentLength,
	TooBig,
}

impl CheckUploadUrlError {
	fn status_code(&self) -> StatusCode {
		match self {
			CheckUploadUrlError::BadUrl => StatusCode::BAD_REQUEST,
			CheckUploadUrlError::Unreachable => StatusCode::GATEWAY_TIMEOUT,
			CheckUploadUrlError::RequestError => StatusCode::FAILED_DEPENDENCY,
			CheckUploadUrlError::NotVideo => StatusCode::UNSUPPORTED_MEDIA_TYPE,
			CheckUploadUrlError::NoContentLength => StatusCode::UNPROCESSABLE_ENTITY,
			CheckUploadUrlError::TooBig => StatusCode::PAYLOAD_TOO_LARGE,
		}
	}

	fn message(&self) -> &str {
		match self {
			CheckUploadUrlError::BadUrl => "url is not valid",
			CheckUploadUrlError::Unreachable => "url is not reachable",
			CheckUploadUrlError::RequestError => "url returned an error",
			CheckUploadUrlError::NotVideo => "url does not point to a video",
			CheckUploadUrlError::NoContentLength => {
				"HEAD does not return a content length"
			}
			CheckUploadUrlError::TooBig => "video is too big",
		}
	}
}

async fn check_upload_url_impl(
	url_string: &str,
	is_premium: bool,
) -> Result<(), CheckUploadUrlError> {
	// check if the url_string is really a url
	match url_string.parse::<Uri>() {
		Ok(x) if x.scheme().is_some() => {
			let scheme = x.scheme().unwrap();
			if scheme != &uri::Scheme::HTTP && scheme != &uri::Scheme::HTTPS {
				return Err(CheckUploadUrlError::BadUrl);
			}
		}
		_ => return Err(CheckUploadUrlError::BadUrl),
	};

	// check the content length with a HEAD request
	// endpoint must respond with a 'Content-Length' or we bail

	// this expression returns an Err if the above doesn't hold
	let response = REQWEST_CLIENT
		.get()
		.await
		.head(url_string)
		.send()
		.await
		.map_err(|_| CheckUploadUrlError::Unreachable)?;

	if !response.status().is_success() {
		return Err(CheckUploadUrlError::RequestError);
	}

	response
		.headers()
		.get(reqwest::header::CONTENT_TYPE)
		.and_then(|x| x.to_str().ok())
		.and_then(|x| x.starts_with("video/").option())
		.ok_or(CheckUploadUrlError::NotVideo)?;

	response
		.content_length()
		.ok_or(CheckUploadUrlError::NoContentLength)?
		.pipe(|l| check_file_size(l as usize, is_premium).option())
		.ok_or(CheckUploadUrlError::TooBig)?;

	Ok(())
}

/// Helper function to load a file from a url
async fn load_file_from_url(url: Uri, is_premium: bool) -> Result<Vec<u8>, StatusCode> {
	let url_string = url.to_string();

	// check_upload_url_impl reparses the url, but it's not a big deal
	// i hope
	if let Err(x) = check_upload_url_impl(&url_string, is_premium).await {
		return Err(x.status_code());
	}

	// now we can download the file
	// reqwest uses `hyper` for http, which, according to https://github.com/seanmonstar/warp/issues/326
	// limits the body size to Content-Length, so we don't need to check the bytes read.
	let res = REQWEST_CLIENT
		.get()
		.await
		.get(&url_string)
		.send()
		.await
		.map_err(|_| CheckUploadUrlError::Unreachable.status_code())?;
	if !res.status().is_success() {
		return Err(CheckUploadUrlError::RequestError.status_code());
	}
	let body =
		res.bytes().await.map_err(|_| CheckUploadUrlError::RequestError.status_code())?;
	Ok(body.into())
}

async fn read_file<T: RemoteFileManager>(
	State(file_manager): State<Arc<T>>,
	Path(file_identifier): Path<String>,
) -> Result<Vec<u8>, (StatusCode, Cow<'static, str>)> {
	const EMPTY_COW: Cow<'static, str> = Cow::Borrowed("");

	let file_identifier: RemoteFileIdentifier =
		file_identifier.parse().map_err(|_| {
			println!("failed to parse file identifier");
			(StatusCode::NOT_FOUND, EMPTY_COW)
		})?;

	let remote_file = file_manager
		.get_file(&file_identifier, RemoteFileKind::VideoInput)
		.await
		.map_err(|_| {
			println!("failed to get file");
			(StatusCode::NOT_FOUND, EMPTY_COW)
		})?;

	let file = file_manager.load_file(&remote_file).await.map_err(|x| match x {
		voice_shared::RemoteFileManagerError::ReadError => {
			(StatusCode::INTERNAL_SERVER_ERROR, "failed to read file".into())
		}
		voice_shared::RemoteFileManagerError::Unspecified(x) => {
			let msg = format!("unspecified error: {:?}", x);
			println!("{}", msg);
			(StatusCode::INTERNAL_SERVER_ERROR, msg.into())
		}
		voice_shared::RemoteFileManagerError::WriteError => {
			unreachable!("write error")
		}
		voice_shared::RemoteFileManagerError::ChildError(_) => {
			unreachable!("child error")
		}
	})?;

	Ok(file)
}
