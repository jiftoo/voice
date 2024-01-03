use std::{
	collections::{HashMap, HashSet},
	env::var,
	sync::Arc,
};

use axum::{
	extract::{Query, State},
	http::{StatusCode, Uri},
	response::{IntoResponse, Redirect},
	routing::get,
	Router,
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
	let router = Router::new()
		.route("/", get(get_waveform))
		.with_state(Arc::new(FileManager::new()));
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

#[derive(serde::Deserialize)]
struct GetWaveformQuery(#[serde(rename = "fileHash")] String);

async fn get_waveform(
	Query(GetWaveformQuery(hash)): Query<GetWaveformQuery>,
	State(waveform_creator): State<Arc<impl WaveformCreator>>,
) -> Result<Redirect, StatusCode> {
	waveform_creator
		.get_waveform_url(hash)
		.map(Redirect::to)
		.map_err(|_| StatusCode::NOT_FOUND)
}

trait WaveformCreator {
	fn get_waveform_url(&mut self, hash: String) -> Uri;
}

struct FileManager {
	cache: HashSet<String>,
}

impl FileManager {
	fn new() -> Self {
		Self { cache: HashSet::new() }
	}
}
