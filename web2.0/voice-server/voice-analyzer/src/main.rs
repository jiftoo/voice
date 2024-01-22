#![forbid(unused_crate_dependencies)]
#![allow(clippy::option_env_unwrap)]

include!("../../include/builder_comperr.rs");

use std::{borrow::Cow, ops::Range, sync::Arc};

use axum::{
	body::Bytes,
	extract::{Path, Query, State},
	http::{HeaderMap, StatusCode},
	routing::{get, post},
	Json, Router,
};
use serde::{ser::SerializeSeq, Deserialize};
use tokio::sync::OnceCell;
use voice_shared::{
	cell_deref::OnceCellDeref, RemoteFileIdentifier, RemoteFileKind, RemoteFileManager,
};

mod analyze;
mod ffmpeg;

pub static CONFIG: OnceCellDeref<voice_shared::config::VoiceAnalyzerConfig> =
	OnceCellDeref::const_new();

#[tokio::main]
async fn main() {
	CONFIG
		.get_or_init(|| async {
			toml::from_str(&std::fs::read_to_string("./config.toml").unwrap()).unwrap()
		})
		.await;

	voice_shared::axum_serve(
		Router::new()
			.route("/file-info", post(get_file_info))
			.route("/analyze/:file_id", get(analyze_video))
			.with_state(voice_shared::yandex_remote::file_manager().await.into()),
		3004,
	)
	.await;
}

#[derive(Deserialize)]
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
	State(file_manager): State<Arc<T>>,
	// manually send a json and set headers to 'application/json'
) -> Result<(HeaderMap, Vec<u8>), StatusCode> {
	let file_identifier: RemoteFileIdentifier =
		file_identifier.parse().map_err(|_| {
			println!("failed to parse file identifier");
			StatusCode::NOT_FOUND
		})?;

	let mut headers = HeaderMap::new();
	headers.insert("Content-Type", "application/json".parse().unwrap());

	// if we had already analyzed this file, return the analysis;
	if let Ok(analysis) = file_manager
		.get_file(&file_identifier, RemoteFileKind::VideoAnalysis(file_identifier))
		.await
	{
		println!("analysis already exists for {}", file_identifier);
		let analysis = file_manager.load_file(&analysis).await.unwrap();
		return Ok((headers, analysis));
	}

	// otherwise load the video file and analyze it.
	let input_file = file_manager
		.get_file(&file_identifier, RemoteFileKind::VideoInput)
		.await
		.map_err(|_| {
			println!("failed to get input file");
			StatusCode::NOT_FOUND
		})?;

	let analysis = ffmpeg::FFmpeg::new(file_manager.file_url(&input_file).await)
		.analyze_silence()
		.await
		.map_err(|_| {
			println!("failed to analyze video");
			StatusCode::INTERNAL_SERVER_ERROR
		})?;

	// new backend "skips" the provided fragments
	let skips_json =
		serde_json::to_string(&RangerSerializer(analysis.inaudible)).unwrap();

	file_manager
		.upload_file(
			skips_json.as_bytes(),
			RemoteFileKind::VideoAnalysis(*input_file.identifier()),
		)
		.await
		.map_err(|_| {
			println!("failed to save skips");
			StatusCode::NOT_FOUND
		})?;

	println!("new analysis for {}", file_identifier);

	Ok((headers, skips_json.into_bytes()))
}
