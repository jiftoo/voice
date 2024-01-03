use std::{
	collections::{HashMap, HashSet},
	env::var,
	sync::Arc,
};

use axum::{
	extract::{Query, State},
	http::{StatusCode, Uri},
	response::{IntoResponse, Redirect},
	routing::{get, post},
	Json, Router,
};
use serde::{Deserialize, Deserializer};
use tokio::net::TcpListener;

use voice_shared::debug_remote;

#[tokio::main]
async fn main() {
	let router = Router::new()
		.route("/", post(upload_file))
		.with_state(Arc::new(debug_remote::DebugRemoteManager::new("./debug_bucket")));
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

#[derive(Debug, serde::Deserialize)]
enum UploadFileBody {
	#[serde(deserialize_with = "deserialize_uri")]
	Url(Uri),
	File(Vec<u8>),
}

fn deserialize_uri<'de, D>(deserializer: D) -> Result<Uri, D::Error>
where
	D: Deserializer<'de>,
{
	String::deserialize(deserializer)?.parse().map_err(serde::de::Error::custom)
}

async fn upload_file(
	State(file_manager): State<Arc<impl voice_shared::RemoteManager>>,
	Json(body): Json<UploadFileBody>,
) -> impl IntoResponse {
	file_manager.upload_file(file);
	todo!()
}
