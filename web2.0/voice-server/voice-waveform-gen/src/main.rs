#![forbid(unused_crate_dependencies)]
#![allow(clippy::option_env_unwrap)]

include!("../../include/builder_comperr.rs");

mod waveform_creator;

use std::sync::Arc;

use axum::{
	extract::{Path, State},
	http::{HeaderMap, StatusCode},
	routing::get,
	Router,
};

use voice_shared::{
	cell_deref::OnceCellDeref, RemoteFileIdentifier, RemoteFileManager, RemoteFileManagerError,
};
use waveform_creator::WaveformCreator;

pub static CONFIG: OnceCellDeref<voice_shared::config::VoiceWaveformGenConfig> =
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
			// since waveforms are unique resources, it's better to use the path to access them
			.route("/:file_id", get(get_waveform))
			.layer(tower_http::cors::CorsLayer::permissive())
			.with_state(Arc::new(WaveformCreator::new(
				voice_shared::yandex_remote::file_manager().await,
			))),
		3003,
	)
	.await;
}

async fn get_waveform<T: RemoteFileManager>(
	State(waveform_creator): State<Arc<WaveformCreator<T>>>,
	Path(file_identifier): Path<String>,
) -> Result<(HeaderMap, Vec<u8>), StatusCode> {
	println!("get waveform route");
	let file_identifier: RemoteFileIdentifier = file_identifier.parse().map_err(|_| {
		println!("failed to parse file identifier");
		StatusCode::NOT_FOUND
	})?;

	// get_waveform already checks if the file identifier is a `RemoteFileKind::Waveform`
	let res = waveform_creator.get_waveform(&file_identifier).await;
	println!("res: {:?}", res.as_ref().map(|x| x.len()));

	match res {
		// Ok(remote_file) => Ok(Redirect::to(remote_file.as_str())),
		Ok(bytes) => {
			let mut headers = HeaderMap::new();
			headers.insert("Content-Disposition", "inline".parse().unwrap());
			headers.insert("Content-Type", "image/png".parse().unwrap());
			headers.insert("Cache-Control", "public, max-age=31536000, immutable".parse().unwrap());
			Ok((headers, bytes))
		}
		Err(RemoteFileManagerError::ReadError) => Err(StatusCode::NOT_FOUND),
		Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
	}
}
