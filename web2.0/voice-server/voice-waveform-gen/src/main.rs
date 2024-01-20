mod waveform_creator;

use std::{
	env::var,
	io::{Read, Write},
	process::{Command, Stdio},
	sync::Arc,
	time::Duration,
};

use axum::{
	extract::{Path, Query, State},
	http::{HeaderMap, StatusCode},
	response::Redirect,
	routing::get,
	Router,
};
use tokio::{io::AsyncWriteExt, net::TcpListener};
use voice_shared::{RemoteFileIdentifier, RemoteFileManager, RemoteFileManagerError};
use waveform_creator::WaveformCreator;

#[tokio::main]
async fn main() {
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
	let file_identifier: RemoteFileIdentifier =
		file_identifier.parse().map_err(|_| {
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
			headers.insert(
				"Cache-Control",
				"public, max-age=31536000, immutable".parse().unwrap(),
			);
			Ok((headers, bytes))
		}
		Err(RemoteFileManagerError::ReadError) => Err(StatusCode::NOT_FOUND),
		Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
	}
}
