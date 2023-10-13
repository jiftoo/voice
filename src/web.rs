use std::borrow::Cow;
use std::collections::HashMap;
use std::io;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Bytes, StreamBody};
use axum::debug_handler;
use axum::extract::multipart::MultipartRejection;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{self, WebSocket};
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State, WebSocketUpgrade};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{
	routing::{get, post},
	Router,
};

use tokio::sync::{RwLock, RwLockReadGuard};
use tokio_util::io::ReaderStream;
use tower_http::cors::{AllowHeaders, AllowOrigin};
use tower_http::services::ServeDir;

use crate::config::CONFIG;
use crate::task::{Task, TaskId, TaskStatus, TaskUpdateMessage};
use crate::{config, task};

struct TaskManager {
	tasks: Arc<RwLock<HashMap<TaskId, task::Task>>>,
}

impl TaskManager {
	fn new() -> Self {
		Self { tasks: Arc::new(RwLock::new(HashMap::new())) }
	}

	async fn new_task(&self, input_data: impl AsRef<[u8]>) -> io::Result<TaskId> {
		let config_lock = CONFIG.read().await;

		let task_id = Task::gen_id();
		let task_id_string = task_id.to_string();

		let input_file_path = config_lock.inputs_dir.join(&task_id_string);

		tokio::fs::write(&input_file_path, input_data).await?;

		let task = Task::new(
			input_file_path,
			task_id,
			&config_lock.outputs_dir,
			config_lock.ffmpeg_executable.clone(),
		)?;

		self.tasks.write().await.insert(task_id, task);

		Ok(task_id)
	}

	async fn cleanup_tasks(&self) {
		let config_lock = CONFIG.read().await;
		let current_time = time::OffsetDateTime::now_utc();
		let mut keys_to_delete = Vec::new();
		let mut tasks_lock = self.tasks.write().await;
		for (id, task) in tasks_lock.iter() {
			let status = task.last_status().await;
			if let TaskStatus::Completed { end_time } = status {
				let to_delete = end_time
					+ time::Duration::minutes(
						config_lock.delete_files_after_minutes as i64,
					) < current_time;
				if to_delete {
					tracing::info!("deleting task {}", id);
					keys_to_delete.push(*id);
				}
			}
		}
		tasks_lock.retain(|k, _| !keys_to_delete.contains(k));
	}

	async fn cleanup_task_files(&self) {
		let config_lock = CONFIG.read().await;
		for dir_entry in config_lock.outputs_dir.read_dir().unwrap().flatten() {
			let file_name = dir_entry.file_name().to_string_lossy().to_string();
			if let Ok(task_id) = file_name.parse::<TaskId>() {
				if !self.tasks.read().await.contains_key(&task_id) {
					tracing::info!("cleaning output file {}", task_id);
					let _ = tokio::fs::remove_file(dir_entry.path()).await;
				}
			}
		}
		for dir_entry in config_lock.inputs_dir.read_dir().unwrap().flatten() {
			let file_name = dir_entry.file_name().to_string_lossy().to_string();
			if let Ok(task_id) = file_name.parse::<TaskId>() {
				if !self.tasks.read().await.contains_key(&task_id) {
					tracing::info!("cleaning input file {}", task_id);
					let _ = tokio::fs::remove_file(dir_entry.path()).await;
				}
			}
		}
	}

	async fn get_task(&self, id: TaskId) -> Option<RwLockReadGuard<Task>> {
		let a = self.tasks.read().await;
		a.get(&id)?;
		Some(RwLockReadGuard::map(a, |x| x.get(&id).unwrap()))
	}
}

#[derive(Clone)]
struct AppState {
	task_manager: Arc<TaskManager>,
}

fn spawn_task_cleaner(task_manager: Arc<TaskManager>) {
	tokio::task::spawn(async move {
		loop {
			tokio::time::sleep(Duration::from_secs(60)).await;
			task_manager.cleanup_tasks().await;
			task_manager.cleanup_task_files().await;
		}
	});
}

pub async fn initialize_server() {
	let app_state: AppState = AppState { task_manager: Arc::new(TaskManager::new()) };

	spawn_task_cleaner(app_state.task_manager.clone());

	let router = Router::new()
		.route("/submit", post(submit))
		.route("/status", get(status))
		.route("/status_ws", get(status_ws))
		.route("/videos/:video", get(videos))
		.fallback_service(ServeDir::new(CONFIG.read().await.web_root.clone()))
		.with_state(app_state)
		.layer(DefaultBodyLimit::max(config::CONFIG.read().await.max_file_size as usize))
		.layer(
			tower_http::cors::CorsLayer::permissive()
				.allow_origin(AllowOrigin::mirror_request())
				.allow_credentials(true)
				.allow_headers(AllowHeaders::mirror_request())
				.allow_methods(["GET".parse().unwrap(), "POST".parse().unwrap()])
				.expose_headers([CONTENT_TYPE, CONTENT_LENGTH]),
		);

	let addr = SocketAddr::from(([0, 0, 0, 0], config::CONFIG.read().await.port));
	tracing::debug!("Started server on {}", addr);
	axum::Server::bind(&addr).serve(router.into_make_service()).await.unwrap();
}

enum EndpointResult<T: IntoResponse> {
	Ok(T),
	Accepted(T),
	Err(StatusCode, Option<Cow<'static, str>>),
}

impl<T: IntoResponse> IntoResponse for EndpointResult<T> {
	fn into_response(self) -> axum::response::Response {
		match self {
			Self::Ok(t) => t.into_response(),
			Self::Accepted(r) => (StatusCode::ACCEPTED, r).into_response(),
			Self::Err(code, msg) => (code, msg.unwrap_or_default()).into_response(),
		}
	}
}

async fn parse_multipart<'a>(multipart: &mut Multipart) -> Result<Bytes, Cow<'a, str>> {
	while let Some(a) = multipart.next_field().await.ok().flatten() {
		let Some(name) = a.name() else {
			return Err("No name for field".into());
		};
		if name == "file" {
			let is_good_mime =
				a.content_type().map(|x| x.starts_with("video/")).unwrap_or(false);
			if is_good_mime {
				return a.bytes().await.map_err(|_| "Failed to read body".into());
			}
		}
	}
	Err("No file field".into())
}

async fn drain_multipart(mut multipart: Multipart) {
	while let Some(mut field) = multipart.next_field().await.ok().flatten() {
		while let Some(x) = field.chunk().await.ok().flatten() {
			// tracing::debug!("drained {} bytes", x.len());
			drop(x);
		}
	}
}

/// Submit a video file to be encoded
/// Accepts a `multipart/form-data` request with a `file` field
///
/// Returns the id of the encoding task, which the client may later query
/// or an error along with an explanation message if the request is malformed.
#[debug_handler]
async fn submit(
	state: State<AppState>,
	multipart: Result<Multipart, MultipartRejection>,
) -> EndpointResult<String> {
	tracing::debug!("submit {:?}", multipart.as_ref().map(|_| ()));
	match multipart {
		Ok(mut multipart) => {
			let input_data = match parse_multipart(&mut multipart).await {
				Ok(x) => x,
				Err(msg) => {
					return EndpointResult::Err(StatusCode::BAD_REQUEST, Some(msg))
				}
			};

			// drain the request so it's possible to send a response
			// in case the client sent multiple fields
			drain_multipart(multipart).await;

			tracing::debug!("Length of file is {} bytes", input_data.len());

			let task_id = match state.task_manager.new_task(input_data).await {
				Ok(task_id) => task_id,
				Err(err) => {
					let err_string = err.to_string();
					tracing::error!("Failed to start task: {}", err_string);
					return EndpointResult::Err(
						StatusCode::INTERNAL_SERVER_ERROR,
						Some(err_string.into()),
					);
				}
			};

			EndpointResult::Accepted(task_id.to_string())
		}
		Err(err) => {
			EndpointResult::Err(StatusCode::BAD_REQUEST, Some(err.to_string().into()))
		}
	}
}

#[derive(serde::Deserialize)]
struct TaskStatusQuery {
	t: TaskId,
}

async fn status(
	state: State<AppState>,
	Query(TaskStatusQuery { t }): Query<TaskStatusQuery>,
) -> EndpointResult<String> {
	let Some(status) = state.task_manager.get_task(t).await else {
		return EndpointResult::Err(StatusCode::NOT_FOUND, Some("task not found".into()));
	};

	EndpointResult::Ok(serde_json::to_string(&status.last_status().await).unwrap())
}

#[debug_handler]
async fn status_ws(
	ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
	state: State<AppState>,
	Query(TaskStatusQuery { t }): Query<TaskStatusQuery>,
) -> EndpointResult<Response> {
	let ws = match ws {
		Ok(x) => x,
		Err(err) => {
			tracing::info!("ws upgrade rejected: {}", err);
			return EndpointResult::Err(err.status(), None);
		}
	};

	// reject upgrade if no task found
	let rx = match state.task_manager.get_task(t).await {
		Some(task) => task.subscribe().await,
		None => {
			tracing::info!("ws upgrade rejected: task not found");
			return EndpointResult::Err(StatusCode::NOT_FOUND, None);
		}
	};

	EndpointResult::Ok(
		ws.on_failed_upgrade(|_| tracing::info!("ws upgrade failed"))
			.on_upgrade(move |ws| ws_handler(ws, t, rx)),
	)
}

async fn ws_handler(
	mut ws: WebSocket,
	target_task: TaskId,
	mut task_rx: tokio::sync::broadcast::Receiver<TaskUpdateMessage>,
) {
	tracing::info!("ws connected");
	if ws.send(ws::Message::Ping(Vec::new())).await.is_err() {
		tracing::info!("can't ping ws");
		return;
	}

	tokio::spawn(async move {
		loop {
			let rcv = task_rx.recv().await;
			match rcv {
				Ok(msg) => {
					if let Err(x) = ws
						.send(ws::Message::Text(serde_json::to_string(&msg.1).unwrap()))
						.await
					{
						tracing::info!(
							"failed to send ws message for {target_task}: {x:?}"
						);
					};

					if matches!(
						msg.1,
						TaskStatus::Error(_) | TaskStatus::Completed { .. }
					) {
						// give axum time to flush the socket
						tokio::time::sleep(Duration::from_millis(500)).await;
						let _ = ws.close().await;
						break;
					}
				}
				Err(tokio::sync::broadcast::error::RecvError::Closed) => {
					tracing::warn!("tx closed for {target_task}");
				}
				Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
					tracing::warn!("rx lagged by {n} for {target_task}");
				}
			}
		}
	});
}

#[derive(serde::Deserialize)]
struct VideoDlQuery {
	dl: u32,
}

async fn videos(
	state: State<AppState>,
	Path(task_id): Path<TaskId>,
	query: Option<Query<VideoDlQuery>>,
) -> EndpointResult<(HeaderMap, StreamBody<ReaderStream<tokio::fs::File>>)> {
	if let Some(task) = state.task_manager.get_task(task_id).await {
		if matches!(task.last_status().await, TaskStatus::InProgress { .. }) {
			return EndpointResult::Err(
				StatusCode::NOT_FOUND,
				Some("video not found".into()),
			);
		}
	}

	let file_path = CONFIG.read().await.outputs_dir.join(task_id.to_string());
	let Ok(file_handle) = tokio::fs::File::open(file_path).await else {
		// usually
		return EndpointResult::Err(
			StatusCode::NOT_FOUND,
			Some("video not found".into()),
		);
	};

	let mut headers = HeaderMap::new();
	headers.append(CONTENT_TYPE, "video/mp4".parse().unwrap());
	headers.append(CONTENT_LENGTH, file_handle.metadata().await.unwrap().len().into());
	if matches!(query, Some(Query(VideoDlQuery { dl: 1 }))) {
		headers.append(CONTENT_DISPOSITION, "attachment".parse().unwrap());
	}

	EndpointResult::Ok((headers, StreamBody::new(ReaderStream::new(file_handle))))
}
