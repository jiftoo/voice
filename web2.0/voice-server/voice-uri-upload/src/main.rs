#![forbid(unused_crate_dependencies)]
#![allow(clippy::option_env_unwrap)]

mod util;

use std::time::Duration;

use axum::{
	body::Bytes,
	extract::{Query, State},
	http::{
		self,
		header::{self},
		HeaderMap, HeaderName, StatusCode, Uri,
	},
	routing::{get, post, put},
	Json, Router,
};

use serde::Deserialize;
use tap::{Pipe, Tap};

use tokio_stream::{Stream, StreamExt};

use crate::util::BooleanOption;

struct ReqwestSingleton(tokio::sync::OnceCell<reqwest::Client>);

impl ReqwestSingleton {
	const fn new() -> Self {
		Self(tokio::sync::OnceCell::const_new())
	}

	async fn get(&self, config: &Config) -> reqwest::Client {
		REQWEST_CLIENT
			.0
			.get_or_try_init(|| async {
				let mut headers = reqwest::header::HeaderMap::new();
				headers.insert(reqwest::header::USER_AGENT, "voice-file-upload".parse().unwrap());
				let client = reqwest::Client::builder()
					.connect_timeout(config.reqwest_connect_timeout)
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

fn check_file_size(file_size: usize, is_premium: bool, config: &Config) -> bool {
	if is_premium {
		file_size <= config.max_premium_file_size
	} else {
		file_size <= config.max_free_file_size
	}
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Config {
	bucket_mount: String,
	silence_cutoff: (i32, i32),   // -90,-10
	skip_duration: (i32, i32),    // 100,250
	max_free_file_size: usize,    // 104857600
	max_premium_file_size: usize, // 367001600
	// the server is not worth our time if it's a slowpoke
	reqwest_connect_timeout: Duration, // 3 secs
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
	use std::env::var;
	let config = Config {
		bucket_mount: var("BUCKET").expect("BUCKET to be set"),
		silence_cutoff: {
			let l = var("SILENCE_CUTOFF_L")
				.expect("SILENCE_CUTOFF_L to be set")
				.parse()
				.expect("SILENCE_CUTOFF_L to be a negative integer");
			let h = var("SILENCE_CUTOFF_H")
				.expect("SILENCE_CUTOFF_H to be set")
				.parse()
				.expect("SILENCE_CUTOFF_H to be a negative integer");
			if l > h {
				panic!("SILENCE_CUTOFF_L ({l}) > SILENCE_CUTOFF_H ({h})")
			}
			(l, h)
		},
		skip_duration: {
			let l = var("SKIP_DURATION_L")
				.expect("SKIP_DURATION_L to be set")
				.parse()
				.expect("SKIP_DURATION_L to be a positive integer");
			let h = var("SKIP_DURATION_H")
				.expect("SKIP_DURATION_H to be set")
				.parse()
				.expect("SKIP_DURATION_H to be a positive integer");
			if l > h {
				panic!("SKIP_DURATION_L ({l}) > SKIP_DURATION_H ({h})")
			}
			(l, h)
		},
		max_free_file_size: var("FREE_SIZE")
			.expect("FREE_SIZE to be set")
			.parse()
			.expect("FREE_SIZE to be a positive integer"),
		max_premium_file_size: var("PREMIUM_SIZE")
			.expect("PREMIUM_SIZE to be set")
			.parse()
			.expect("PREMIUM_SIZE to be a positive integer"),
		reqwest_connect_timeout: {
			let x: u64 = var("CONN_TIMEOUT_MS")
				.expect("CONN_TIMEOUT_MS to be set")
				.parse()
				.expect("CONN_TIMEOUT_MS to be a positive integer");
			Duration::from_millis(x)
		},
	};

	voice_shared::axum_serve(
		Router::new()
			.route("/check-upload-url", put(check_upload_url))
			.route("/constants", get(get_constants))
			.route("/upload-by-url", post(upload_file))
			.with_state(config),
	)
	.await;
}

#[derive(Deserialize)]
struct IsPremiumQuery {
	premium: bool,
}

async fn get_constants(
	State(config): State<Config>,
	Query(IsPremiumQuery { premium }): Query<IsPremiumQuery>,
) -> ([(HeaderName, &'static str); 1], Json<serde_json::Value>) {
	let json = serde_json::json!({
		"silenceCutoff": {
			"min": config.silence_cutoff.0,
			"max": config.silence_cutoff.1,
		},
		"skipDuration": {
			"min": config.skip_duration.0,
			"max": config.skip_duration.1,
		},
		"maxFileSize": if premium { config.max_premium_file_size } else { config.max_free_file_size },
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
	State(config): State<Config>,
	Query(IsPremiumQuery { premium }): Query<IsPremiumQuery>,
	body: String,
) -> Result<(), (StatusCode, String)> {
	check_upload_url_impl(&body, premium, &config)
		.await
		.map_err(|x| (x.status_code(), x.message().to_owned()))
		.tap(|x| println!("check upload url ({}) result: {:?}", body, x))
}

#[derive(Debug)]
enum InvalidUriWrapper {
	Standard(http::uri::InvalidUri),
	NotHttpOrHttps,
}

fn check_uri(input: &str) -> Result<Uri, InvalidUriWrapper> {
	let uri: Uri = input.parse().map_err(InvalidUriWrapper::Standard)?;

	if !matches!(uri.scheme_str(), Some("http" | "https")) {
		return Err(InvalidUriWrapper::NotHttpOrHttps);
	}

	Ok(uri)
}

fn generate_file_id() -> String {
	hex::encode(std::iter::repeat_with(rand::random).take(32).collect::<Vec<u8>>())
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
async fn upload_file(
	State(config): State<Config>,
	headers: HeaderMap,
	uri: String,
) -> Result<String, (StatusCode, String)> {
	let uri = match check_uri(&uri) {
		Ok(x) => x,
		Err(err) => return Err((StatusCode::BAD_REQUEST, format!("url is malformed: {err:?}"))),
	};

	println!("uploading_file from {uri}, x-premium: {}", headers.get("x-premium").is_some());
	let Some(content_type) = headers.get(header::CONTENT_TYPE).and_then(|x| x.to_str().ok()) else {
		return Err((StatusCode::BAD_REQUEST, "content-type header is required".to_owned()));
	};

	// TODO: replace with actual authenfication when it's ready
	let is_premium = headers.get("x-premium").is_some();

	let mut download = match content_type {
		"text/x-url" => {
			if let Some(true) = uri
				.scheme()
				.map(|x| x == &http::uri::Scheme::HTTP || x == &http::uri::Scheme::HTTPS)
			{
				load_file_from_url(uri, is_premium, &config).await.map_err(|_| {
					(StatusCode::UNPROCESSABLE_ENTITY, "error when reaching url".to_owned())
				})?
			} else {
				return Err((
					StatusCode::BAD_REQUEST,
					"scheme should be 'http' or 'https'".to_owned(),
				));
			}
		}
		_ => {
			return Err((StatusCode::BAD_REQUEST, "content type should be 'text/x-url'".to_owned()))
		}
	};

	let file_id = generate_file_id();
	let file_path: &std::path::Path = config.bucket_mount.as_ref();
	let file_path = file_path.join(file_id.clone());

	let mut remote_file = tokio::fs::File::create(&file_path).await.map_err(|x| {
		println!("error while creating file: {x}");
		(StatusCode::INTERNAL_SERVER_ERROR, "failed to create file".to_owned())
	})?;

	while let Some(chunk) = download.next().await {
		match chunk {
			Ok(chunk) => {
				tokio::io::copy(&mut chunk.as_ref(), &mut remote_file)
					.await
					.expect("saving a chunk to file to succeed");
			}
			Err(e) => {
				println!("error while saving file: {e}");
				drop(remote_file);
				tokio::fs::remove_file(&file_path).await.expect("deleting file to succeed");
				break;
			}
		}
	}

	println!("saved file: {:?}", file_path);

	Ok(file_id)
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
			CheckUploadUrlError::NoContentLength => "HEAD does not return a content length",
			CheckUploadUrlError::TooBig => "video is too big",
		}
	}
}

async fn check_upload_url_impl(
	url_string: &str,
	is_premium: bool,
	config: &Config,
) -> Result<(), CheckUploadUrlError> {
	// check if the url_string is really a url
	match url_string.parse::<Uri>() {
		Ok(x) if x.scheme().is_some() => {
			let scheme = x.scheme().unwrap();
			if scheme != &http::uri::Scheme::HTTP && scheme != &http::uri::Scheme::HTTPS {
				return Err(CheckUploadUrlError::BadUrl);
			}
		}
		_ => return Err(CheckUploadUrlError::BadUrl),
	};

	// check the content length with a HEAD request
	// endpoint must respond with a 'Content-Length' or we bail

	// this expression returns an Err if the above doesn't hold
	let response = REQWEST_CLIENT
		.get(config)
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
		// .content_length() // content_length won't work here
		.headers()
		.get(reqwest::header::CONTENT_LENGTH)
		.and_then(|x| x.to_str().ok())
		.and_then(|x| x.parse::<usize>().ok())
		.ok_or(CheckUploadUrlError::NoContentLength)?
		.pipe(|l| check_file_size(l, is_premium, config).option())
		.ok_or(CheckUploadUrlError::TooBig)?;

	Ok(())
}

/// Helper function to load a file from a url
async fn load_file_from_url(
	url: Uri,
	is_premium: bool,
	config: &Config,
) -> Result<impl Stream<Item = Result<Bytes, reqwest::Error>>, StatusCode> {
	let url_string = url.to_string();

	// check_upload_url_impl reparses the url, but it's not a big deal
	// i hope
	if let Err(x) = check_upload_url_impl(&url_string, is_premium, config).await {
		return Err(x.status_code());
	}

	// now we can download the file
	// reqwest uses `hyper` for http, which, according to https://github.com/seanmonstar/warp/issues/326
	// limits the body size to Content-Length, so we don't need to check the bytes read.
	let res = REQWEST_CLIENT
		.get(config)
		.await
		.get(&url_string)
		.send()
		.await
		.map_err(|_| CheckUploadUrlError::Unreachable.status_code())?;
	if !res.status().is_success() {
		return Err(CheckUploadUrlError::RequestError.status_code());
	}

	Ok(res.bytes_stream())
}
