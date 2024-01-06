mod util;

use std::{
	collections::{HashMap, HashSet},
	env::var,
	str::FromStr,
	sync::Arc,
};

use axum::{
	body::Bytes,
	extract::{FromRequestParts, Query, State},
	http::{
		header::{self, CONTENT_TYPE},
		request::Parts,
		uri, HeaderMap, HeaderName, HeaderValue, StatusCode, Uri,
	},
	response::{IntoResponse, Redirect},
	routing::{get, post},
	Json, Router,
};
use serde::{Deserialize, Deserializer};
use tokio::net::TcpListener;

use voice_shared::{debug_remote, RemoteFileKind, RemoteFileManager};

#[tokio::main]
async fn main() {
	let router = Router::new()
		.route("/", post(upload_file))
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

// #[axum::debug_handler]
async fn upload_file<T: RemoteFileManager>(
	State(file_manager): State<Arc<T>>,
	headers: HeaderMap,
	body: Bytes,
) -> Result<(), StatusCode> {
	let Some(content_type) = headers.get(header::CONTENT_TYPE) else {
		return Err(StatusCode::BAD_REQUEST);
	};

	let file = match content_type.to_str().map_err(|_| StatusCode::BAD_REQUEST)? {
		"application/octet-stream" => body.to_vec(),
		"text/x-url" => {
			let uri: Uri = std::str::from_utf8(&body)
				.ok()
				.and_then(|x| x.parse().ok())
				.ok_or(StatusCode::BAD_REQUEST)?;
			if let Some(true) =
				uri.scheme().map(|x| x == &uri::Scheme::HTTP || x == &uri::Scheme::HTTPS)
			{
				load_file_from_url(uri)
					.await
					.map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?
			} else {
				return Err(StatusCode::BAD_REQUEST);
			}
		}
		_ => return Err(StatusCode::BAD_REQUEST),
	};

	let remote_file = file_manager
		.upload_file(&file, RemoteFileKind::VideoInput)
		.await
		.map_err(|_| StatusCode::BAD_REQUEST)?;

	println!("uploaded file: {:?}", remote_file);

	todo!()
}

async fn load_file_from_url(url: Uri) -> Result<Vec<u8>, StatusCode> {
	let mut headers = reqwest::header::HeaderMap::new();
	headers.insert(reqwest::header::USER_AGENT, "voice-file-upload".parse().unwrap());
	let client = reqwest::Client::builder()
		.default_headers(headers)
		.build()
		.map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
	let res = client
		.get(url.to_string())
		.send()
		.await
		.map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
	if !res.status().is_success() {
		return Err(StatusCode::UNPROCESSABLE_ENTITY);
	}
	let body = res.bytes().await.map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
	Ok(body.into())
}
