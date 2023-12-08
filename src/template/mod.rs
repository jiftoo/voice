use askama::Template;
use axum::{response::IntoResponse, routing::get, Router};

pub fn routes() -> Router {
	Router::new().route("/", get(index)).route("/completed", get(completed))
}

#[derive(Template)]
#[template(path = "index.html")]
struct Index {}

async fn index() -> impl IntoResponse {
	Index {}
}

async fn completed() -> impl IntoResponse {}
