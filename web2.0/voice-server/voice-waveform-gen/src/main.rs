mod waveform_creator;

use std::{env::var, sync::Arc};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Redirect,
    routing::get,
    Router,
};
use tokio::net::TcpListener;
use voice_shared::{RemoteFileIdentifier, RemoteFileManager, RemoteFileManagerError};
use waveform_creator::WaveformCreator;

#[tokio::main]
async fn main() {
    let router = Router::new()
        .route("/", get(get_waveform))
        .with_state(Arc::new(WaveformCreator::new(voice_shared::debug_remote::file_manager())));
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

async fn get_waveform<T: RemoteFileManager>(
    Query(GetWaveformQuery(hash)): Query<GetWaveformQuery>,
    State(waveform_creator): State<Arc<WaveformCreator<T>>>,
) -> Result<Redirect, StatusCode> {
    let file_identifier: RemoteFileIdentifier =
        hash.parse().map_err(|_| StatusCode::BAD_REQUEST)?;

    match waveform_creator.get_waveform(&file_identifier).await {
        Ok(remote_file) => Ok(Redirect::to(remote_file.as_ref())),
        Err(RemoteFileManagerError::ReadError) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
