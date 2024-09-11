#![forbid(unused_crate_dependencies)]
#![allow(clippy::option_env_unwrap)]

use std::{ops::Range, sync::Arc};

use axum::{
	body::Bytes,
	extract::{Path, Query, State},
	http::{HeaderMap, StatusCode},
	routing::{get, post},
	Json, Router,
};
use ffmpeg::FFmpegError;
use serde::ser::SerializeSeq;

use voice_shared::{RemoteFileIdentifier, RemoteFileKind, RemoteFileManager};

mod analyze;
mod ffmpeg;

pub struct Config {
	pub bucket_mount: String,
	pub silencedetect_duration: String,
	pub silencedetect_noise: String,
}

struct AppState<T: RemoteFileManager> {
	inner: Arc<Inner<T>>,
}

struct Inner<T: RemoteFileManager> {
	config: Config,
	file_manager: T,
}

impl<T: RemoteFileManager> Clone for AppState<T> {
	fn clone(&self) -> Self {
		Self { inner: self.inner.clone() }
	}
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
	let config = Config {
		bucket_mount: std::env::var("BUCKET").expect("BUCKET environment variable to be set"),
		silencedetect_noise: {
			const DB_RANGE: std::ops::Range<i32> = -100..0;
			let duration =
				std::env::var("SILENCEDETECT_NOISE").expect("SILENCEDETECT_NOISE to be set");
			let duration: i32 =
				duration.parse().expect("SILENCEDETECT_NOISE to be a negative integer");
			let duration = DB_RANGE
				.contains(&duration)
				.then_some(duration)
				.expect("SILENCEDETECT_NOISE to be a negative integer");

			format!("{duration}dB")
		},
		silencedetect_duration: {
			let noise =
				std::env::var("SILENCEDETECT_DURATION").expect("SILENCEDETECT_DURATION to be set");
			noise.parse::<f64>().expect("SILENCEDETECT_DURATION to be a float");

			noise
		},
	};

	let file_manager =
		voice_shared::yandex_mount_remote::file_manager(config.bucket_mount.clone()).await;

	let state = AppState { inner: Arc::new(Inner { config, file_manager }) };

	voice_shared::axum_serve(
		Router::new()
			.route("/file-info", post(get_file_info))
			.route("/analyze/:file_id", get(analyze_video))
			.with_state(state.clone())
			.nest(
				"/",
				voice_shared::trigger::trigger_listener(move |x| on_trigger(x, state.clone())),
			),
	)
	.await;
}

fn on_trigger(
	file_identifier: RemoteFileIdentifier,
	app: AppState<impl RemoteFileManager + 'static>,
) {
	tokio::spawn(async move {
		analyze_video(Path(file_identifier.to_string()), State(app))
			.await
			.expect("triggered analyze_video to succeed");
	});
}

#[derive(serde::Deserialize)]
struct IsPremiumQuery {
	premium: bool,
}

#[axum::debug_handler]
async fn get_file_info(
	Query(IsPremiumQuery { premium }): Query<IsPremiumQuery>,
	body: Bytes,
) -> Result<Json<analyze::VideoInfo>, (StatusCode, String)> {
	let info = analyze::get_video_info(
		&body,
		if premium { analyze::Bounds::premium() } else { analyze::Bounds::normal() },
	)
	.await;

	match info {
		Err(x) => Err((StatusCode::INTERNAL_SERVER_ERROR, x.to_string())),
		Ok(analyze::VideoValidity::Valid(x)) => Ok(Json(x)),
		Ok(x) => Err((StatusCode::BAD_REQUEST, x.to_string())),
	}
}

struct RangerSerializer(Vec<Range<f32>>);

// serialize a vector of ranges as an array of json tuples
impl serde::Serialize for RangerSerializer {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
		for x in &self.0 {
			seq.serialize_element(&[x.start, x.end])?;
		}
		seq.end()
	}
}

async fn analyze_video<T: RemoteFileManager>(
	Path(file_identifier): Path<String>,
	State(app): State<AppState<T>>,
	// manually send a json and set headers to 'application/json'
) -> Result<(HeaderMap, Vec<u8>), (StatusCode, String)> {
	let file_identifier: RemoteFileIdentifier = file_identifier.parse().map_err(|e| {
		println!("failed to parse file identifier");
		(StatusCode::NOT_FOUND, "failed to parse file identifier".to_owned())
	})?;

	let mut headers = HeaderMap::new();
	headers.insert("Content-Type", "application/json".parse().unwrap());

	// if we had already analyzed this file, return the analysis;
	if let Ok(analysis) = app
		.inner
		.file_manager
		.get_file(&file_identifier, RemoteFileKind::VideoAnalysis(file_identifier))
		.await
	{
		println!("analysis already exists for {}", file_identifier);
		let analysis = app.inner.file_manager.load_file(&analysis).await.unwrap();
		return Ok((headers, analysis));
	}

	// otherwise load the video file and analyze it.
	let input_file = app
		.inner
		.file_manager
		.get_file(&file_identifier, RemoteFileKind::VideoInput(file_identifier))
		.await
		.map_err(|e| {
			let msg = format!("{e:?}");
			println!("failed to get input file: {msg}");
			(StatusCode::NOT_FOUND, msg)
		})?;

	let analysis = ffmpeg::FFmpeg::new(app.inner.file_manager.file_url(&input_file).await)
		.analyze_silence(&app.inner.config)
		.await
		.map_err(|e| match e {
			FFmpegError::NoSilence => {
				println!("video has no silence");
				(StatusCode::NO_CONTENT, "".to_owned())
			}
			e => {
				let msg = format!("{e:?}");
				println!("failed to analyze video: {msg}");
				(StatusCode::INTERNAL_SERVER_ERROR, msg)
			}
		})?;

	// new frontend "skips" the provided fragments
	let skips_json = serde_json::to_string(&RangerSerializer(analysis.inaudible)).unwrap();

	app.inner
		.file_manager
		.upload_file(skips_json.as_bytes(), RemoteFileKind::VideoAnalysis(*input_file.identifier()))
		.await
		.map_err(|e| {
			let msg = format!("{e:?}");
			println!("failed to save skips: {msg}");
			(StatusCode::NOT_FOUND, msg)
		})?;

	println!("new analysis for {}", file_identifier);

	Ok((headers, skips_json.into_bytes()))
}
