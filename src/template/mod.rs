use axum::{Router, routing::get};

pub fn routes() -> Router {
	Router::new().route("/", get(index)).route("/completed", get(completed))
}
