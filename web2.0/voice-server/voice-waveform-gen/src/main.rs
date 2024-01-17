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
	// let what = std::fs::read("wave.png").unwrap();
	// let what = &what;
	// println!("{}", what.len());

	// let mut to = Command::new("magick");
	// to.stdin(Stdio::piped())
	// 	.stdout(Stdio::piped())
	// 	.stderr(Stdio::inherit())
	// 	.args(["-", "-trim", "-"]);

	// let mut handle = to.spawn().unwrap();
	// let mut stdin = handle.stdin.take().unwrap();
	// // let mut stdout = handle.stdout.take().unwrap();

	// println!("1");
	// // tokio::time::sleep(Duration::from_secs(1)).await;
	// // std::thread::sleep(Duration::from_secs(1));
	// stdin.write_all(what).unwrap();
	// drop(stdin);
	// let output = handle.wait_with_output().unwrap();
	// // let a = handle.stdout.take().unwrap().read_to_end(&mut Vec::new());
	// // let a = stdout.read_to_end(&mut Vec::new()).unwrap();
	// println!("2");

	let router = Router::new()
		// since waveforms are unique resources, it's better to use the path to access them
		.route("/:file_id", get(get_waveform))
		.layer(tower_http::cors::CorsLayer::permissive())
		.with_state(Arc::new(WaveformCreator::new(
			voice_shared::debug_remote::file_manager(),
		)));
	axum::serve(
		TcpListener::bind((
			"0.0.0.0",
			dbg!(var("PORT").map(|x| x.parse().unwrap()).unwrap_or(3003)),
		))
		.await
		.unwrap(),
		router,
	)
	.await
	.unwrap();
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
