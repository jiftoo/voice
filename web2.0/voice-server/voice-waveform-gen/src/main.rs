// #![forbid(unused_crate_dependencies)]
#![allow(clippy::option_env_unwrap)]

mod waveform_creator;

use std::{ops::Deref, process::Stdio, sync::Arc};

use axum::{
	extract::{Path, State},
	http::{HeaderMap, StatusCode},
	routing::get,
	Router,
};

use voice_shared::{RemoteFileIdentifier, RemoteFileManager, RemoteFileManagerError};
use waveform_creator::WaveformCreator;

pub struct Config {
	pub bucket_mount: String,
	pub waveform_dimensions: String,
}

struct AppState<T: RemoteFileManager> {
	inner: Arc<Inner<T>>,
}

struct Inner<T: RemoteFileManager> {
	config: Config,
	creator: WaveformCreator<T>,
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
		waveform_dimensions: {
			let dim = std::env::var("WAVEFORM_DIMENSIONS")
				.expect("WAVEFORM_DIMENSIONS environment variable to be set");
			let split: [&str; 2] = dim
				.split('x')
				.collect::<Vec<_>>()
				.try_into()
				.unwrap_or_else(|_| panic!("WAVEFORM_DIMENSIONS to be an AAAAxBBBB: {dim}"));
			if !split.into_iter().all(|x| x.parse::<u32>().is_ok()) {
				panic!("WAVEFORM_DIMENSIONS is malformed: {dim}")
			}

			dim
		},
	};

	let creator = WaveformCreator::new(
		voice_shared::yandex_mount_remote::file_manager(config.bucket_mount.clone()).await,
	);

	let state = AppState { inner: Arc::new(Inner { config, creator }) };

	voice_shared::axum_serve(
		// since waveforms are unique resources, it's better to use the path to access them
		Router::new()
			.route("/:file_id", get(get_waveform))
			.layer(tower_http::cors::CorsLayer::permissive())
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
		get_waveform(Path(file_identifier.to_string()), State(app))
			.await
			.expect("triggered analyze_video to succeed");
	});
}

async fn get_waveform<T: RemoteFileManager>(
	Path(file_identifier): Path<String>,
	State(app): State<AppState<T>>,
) -> Result<(HeaderMap, Vec<u8>), StatusCode> {
	println!("get waveform route");
	let file_identifier: RemoteFileIdentifier = file_identifier.parse().map_err(|_| {
		println!("failed to parse file identifier");
		StatusCode::NOT_FOUND
	})?;

	// get_waveform already checks if the file identifier is a `RemoteFileKind::Waveform`
	let res = app.inner.creator.get_waveform(&file_identifier, &app.inner.config).await;
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
