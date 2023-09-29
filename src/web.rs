use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::body::StreamBody;
use axum::debug_handler;
use axum::extract::{DefaultBodyLimit, Multipart, Query, State};
use axum::http::header::{CONTENT_TYPE, CONTENT_DISPOSITION, CONTENT_LENGTH};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{
	routing::{get, post},
	Router,
};
use tokio::sync::RwLock;
use tokio_util::io::ReaderStream;

use crate::config::CONFIG;
use crate::task::Running;
use crate::{config, task, video};

#[derive(Clone)]
struct AppState {
	tasks: Arc<RwLock<Vec<task::EncodingTask<Running>>>>,
}

pub async fn initialize_server() {
	let tasks: AppState = AppState { tasks: Arc::new(RwLock::new(Vec::new())) };

	// task task task...
	let poll_thread = tokio::task::spawn({
		let tasks = tasks.tasks.clone();
		async move {
			loop {
				let mut tasks_guard = tasks.write().await;
				for task in tasks_guard.iter_mut() {
					task.poll();
					tokio::time::sleep(Duration::from_millis(100)).await;
				}
			}
		}
	});

	let router = Router::new()
		.route("/submit", post(submit))
		.route("/status", get(status))
		.route("/download", get(download))
		.route("/*0", get(|| async { "Hello, World!" }))
		.with_state(tasks)
		.layer(DefaultBodyLimit::max(config::CONFIG.read().max_file_size as usize));

	let addr = SocketAddr::from(([127, 0, 0, 1], config::CONFIG.read().port));
	tracing::debug!("Started server on {}", addr);
	axum::Server::bind(&addr).serve(router.into_make_service()).await.unwrap();
}

async fn submit(state: State<AppState>, mut data: Multipart) -> impl IntoResponse {
	while let Some(field) = data.next_field().await.unwrap() {
		let name = field.name().unwrap().to_string();
		if name == "file" {
			let data = field.bytes().await.unwrap();
			println!("Length of `{}` is {} bytes", name, data.len());

			let name = format!(
				"{}.mp4",
				SystemTime::now()
					.duration_since(SystemTime::UNIX_EPOCH)
					.unwrap()
					.as_millis()
			);
			let file_handle =
				video::VideoFileHandle::from_file(&data, &name).await.unwrap();

			let name = file_handle.temp.file_name().unwrap().to_str().unwrap().to_owned();

			let task = task::EncodingTask::new(file_handle).start();
			state.tasks.write().await.push(task);

			return (StatusCode::OK, Cow::from(name));
		}
	}
	(StatusCode::BAD_REQUEST, "".into())
}

async fn status(state: State<AppState>) -> impl IntoResponse {
	format!("{:#?}", state.tasks.read().await)
}

#[derive(serde::Deserialize)]
struct DownloadQuery {
	file: String,
}

#[debug_handler]
async fn download(query: Query<DownloadQuery>) -> impl IntoResponse {
	let filtered_file =
		query.file.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>();
	
	let filename = CONFIG.read().temp_dir_root.join(filtered_file).join("output.mp4");
	tracing::debug!("try download filename: {}", filename.display());

	let Ok(file) = tokio::fs::File::open(filename).await else {
		return Err((StatusCode::NOT_FOUND, "Not found"));
	};

	let file_len = file.metadata().await.unwrap().len();
	let stream = ReaderStream::new(file);

	let body = StreamBody::new(stream);

	let mut headers = HeaderMap::new();
	headers.append(CONTENT_TYPE, "video/mp4".parse().unwrap());
	headers.append(
		CONTENT_DISPOSITION,
		"attachment; filename=\"test.mp4\"".parse().unwrap(),
	);
	headers.append(CONTENT_LENGTH, file_len.into());


	Ok((StatusCode::OK, headers, body))
}
