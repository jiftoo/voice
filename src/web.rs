use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::body::{Body, Bytes, StreamBody};
use axum::debug_handler;
use axum::extract::multipart::{Field, MultipartError, MultipartRejection};
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{self, CloseFrame, WebSocket};
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State, WebSocketUpgrade};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::{
	routing::{get, post},
	Router,
};
use axum_typed_multipart::{FieldData, TryFromMultipart, TypedMultipart, TypedMultipartError};
use tokio::io::AsyncRead;
use tokio::sync::{RwLock, RwLockReadGuard};
use tokio_util::io::ReaderStream;
use tower_http::cors::{AllowHeaders, AllowOrigin};

use crate::config::CONFIG;
use crate::task::{Task, TaskId, TaskStatus, TaskUpdateMessage, TaskUpdateSender};
use crate::{config, task};

struct TaskManager {
	tasks: Arc<RwLock<HashMap<TaskId, task::Task>>>,
}

impl TaskManager {
	fn new() -> Self {
		Self {
			tasks: Arc::new(RwLock::new(HashMap::new())),
		}
	}

	async fn new_task(&self, input_data: impl AsRef<[u8]>) -> io::Result<TaskId> {
		let config_lock = CONFIG.read().await;

		let task_id = Task::gen_id();
		let task_id_string = task_id.to_string();

		let input_file_path = config_lock.inputs_dir.join(&task_id_string);

		tokio::fs::write(&input_file_path, input_data).await?;

		let task = Task::new(input_file_path, task_id, &config_lock.outputs_dir, config_lock.ffmpeg_executable.clone())
			.await?;

		self.tasks.write().await.insert(task_id, task);

		Ok(task_id)
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

pub async fn initialize_server() {
	let app_state: AppState = AppState {
		task_manager: Arc::new(TaskManager::new()),
	};

	let router = Router::new()
		.route("/submit", post(submit))
		.route("/status", get(status))
		.route("/status_ws", get(status_ws))
		.route("/videos/:video", get(videos))
		.route("/*0", get(|| async { "Fallback route" }))
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

	let addr = SocketAddr::from(([127, 0, 0, 1], config::CONFIG.read().await.port));
	tracing::debug!("Started server on {}", addr);
	axum::Server::bind(&addr)
		.serve(router.into_make_service())
		.await
		.unwrap();
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
			let is_good_mime = a.content_type().map(|x| x.starts_with("video/")).unwrap_or(false);
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
async fn submit(state: State<AppState>, multipart: Result<Multipart, MultipartRejection>) -> EndpointResult<String> {
	tracing::debug!("submit {:?}", multipart.as_ref().map(|_| ()));
	match multipart {
		Ok(mut multipart) => {
			let input_data = match parse_multipart(&mut multipart).await {
				Ok(x) => x,
				Err(msg) => return EndpointResult::Err(StatusCode::BAD_REQUEST, Some(msg)),
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
					return EndpointResult::Err(StatusCode::INTERNAL_SERVER_ERROR, Some(err_string.into()));
				}
			};

			EndpointResult::Accepted(task_id.to_string())
		}
		Err(err) => EndpointResult::Err(StatusCode::BAD_REQUEST, Some(err.to_string().into())),
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

	EndpointResult::Ok(status.last_status().await.to_string())
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
	EndpointResult::Ok(
		ws.on_failed_upgrade(|_| tracing::info!("ws upgrade failed"))
			.on_upgrade(move |ws| ws_handler(ws, state, t)),
	)
}

async fn ws_handler(mut ws: WebSocket, state: State<AppState>, target_task: TaskId) {
	tracing::info!("ws connected");
	if ws.send(ws::Message::Ping(Vec::new())).await.is_err() {
		tracing::info!("can't ping ws");
		return;
	}

	let Some(mut rx) = state.task_manager.get_task(target_task).await.map(|x| x.subscribe()) else {
		let _ = ws
			.send(ws::Message::Close(Some(CloseFrame {
				code: 0,
				reason: "task not found".into(),
			})))
			.await;
		tracing::info!("ws task not found {}", target_task);
		return;
	};

	tokio::spawn(async move {
		while let Ok(msg) = rx.recv().await {
			let mut abort = false;
			let message = match msg.1 {
				Ok(status @ (TaskStatus::Error(_) | TaskStatus::Completed)) => {
					abort = true;
					status.to_string()
				}
				Err(error) => {
					abort = true;
					format!("Error({error})")
				}
				Ok(status) => status.to_string(),
			};
			let Ok(_) = ws.send(ws::Message::Text(message)).await else {
				tracing::info!("failed to send ws message");
				break;
			};

			if abort {
				break;
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
		if matches!(task.last_status().await, TaskStatus::InProgress(_)) {
			return EndpointResult::Err(StatusCode::NOT_FOUND, Some("video not found".into()));
		}
	}

	let file_path = CONFIG.read().await.outputs_dir.join(task_id.to_string());
	let Ok(file_handle) = tokio::fs::File::open(file_path).await else {
		// usually
		return EndpointResult::Err(StatusCode::NOT_FOUND, Some("video not found".into()));
	};

	let mut headers = HeaderMap::new();
	headers.append(CONTENT_TYPE, "video/mp4".parse().unwrap());
	headers.append(CONTENT_LENGTH, file_handle.metadata().await.unwrap().len().into());
	if matches!(query, Some(Query(VideoDlQuery { dl: 1 }))) {
		headers.append(CONTENT_DISPOSITION, "attachment".parse().unwrap());
	}

	EndpointResult::Ok((headers, StreamBody::new(ReaderStream::new(file_handle))))
}
