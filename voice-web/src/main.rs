mod templates;

use std::{net::SocketAddr, time::Duration};

use askama::Template;
use askama_axum::IntoResponse;
use axum::{
	extract::{ws::rejection::WebSocketUpgradeRejection, WebSocketUpgrade},
	http::StatusCode,
	routing::{get, post},
	Router,
};
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() {
	let pages = Router::new().route("/", get(root));
	let static_files = ServeDir::new("static");
	let api = Router::new().route("/hotreload", get(hotreload));

	let app = pages.nest_service("/static", static_files).nest("/api", api);

	let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
	axum::Server::bind(&addr).serve(app.into_make_service()).await.unwrap();
}

async fn root() -> impl IntoResponse {
	templates::Index::new()
}

async fn hotreload(
	ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> impl IntoResponse {
	ws.unwrap().on_upgrade(|socket| async move {
		tokio::spawn(async move {
			let socket = socket;
			loop {
				tokio::time::sleep(Duration::from_secs(9999)).await;
			}
		});
	})
}
