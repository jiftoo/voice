use std::borrow::Cow;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::body::{Bytes, StreamBody};
use axum::debug_handler;
use axum::extract::multipart::{Field, MultipartError, MultipartRejection};
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{self, CloseFrame, WebSocket};
use axum::extract::{DefaultBodyLimit, Multipart, Query, State, WebSocketUpgrade};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect};
use axum::{
	routing::{get, post},
	Router,
};
use axum_typed_multipart::{
	FieldData, TryFromMultipart, TypedMultipart, TypedMultipartError,
};
use tokio::sync::RwLock;
use tokio_util::io::ReaderStream;
use tower_http::cors::{AllowHeaders, AllowOrigin};

use crate::config::CONFIG;
use crate::task::{Task, TaskId, TaskProgress, TaskUpdateMessage};
use crate::{config, task};

type TaskUpdateSender = tokio::sync::broadcast::Sender<TaskUpdateMessage>;

#[derive(Clone)]
struct AppState {
	tasks: Arc<RwLock<HashMap<TaskId, task::Task>>>,
	task_update_senders: Arc<RwLock<HashMap<TaskId, TaskUpdateSender>>>,
}

pub async fn initialize_server() {
	let app_state: AppState = AppState {
		tasks: Arc::new(RwLock::new(HashMap::new())),
		task_update_senders: Arc::new(RwLock::new(HashMap::new())),
	};

	let router = Router::new()
		.route("/submit", post(submit))
		.route("/status", get(status))
		.route("/status_ws", get(status_ws))
		.route("/download", get(download))
		.route("/*0", get(|| async { "Hello, World!" }))
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
	axum::Server::bind(&addr).serve(router.into_make_service()).await.unwrap();
}

enum EndpointResult<T: IntoResponse> {
	Ok(T),
	Redirect(Redirect),
	Err(StatusCode, Option<Cow<'static, str>>),
}

impl<T: IntoResponse> IntoResponse for EndpointResult<T> {
	fn into_response(self) -> axum::response::Response {
		match self {
			Self::Ok(t) => t.into_response(),
			Self::Redirect(r) => r.into_response(),
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
	mut state: State<AppState>,
	multipart: Result<Multipart, MultipartRejection>,
) -> EndpointResult<String> {
	tracing::debug!("submit {:?}", multipart.as_ref().map(|_| ()));
	match multipart {
		Ok(mut multipart) => {
			let file = match parse_multipart(&mut multipart).await {
				Ok(x) => x,
				Err(msg) => {
					return EndpointResult::Err(StatusCode::BAD_REQUEST, Some(msg))
				}
			};

			// drain the request so it's possible to send a response
			// in case the client sent multiple fields
			drain_multipart(multipart).await;

			tracing::debug!("Length of file is {} bytes", file.len());

			let config_lock = CONFIG.read().await;

			let task_id = Task::gen_id();
			let task_id_string = task_id.to_string();

			let input_file_path = config_lock.inputs_dir.join(&task_id_string);

			if let Err(err) = tokio::fs::write(&input_file_path, file).await {
				let err_string = err.to_string();
				tracing::error!("Failed to save input file: {}", err_string);
				return EndpointResult::Err(
					StatusCode::INTERNAL_SERVER_ERROR,
					Some(err_string.into()),
				);
			}

			let update_tx = tokio::sync::broadcast::channel::<TaskUpdateMessage>(8).0;
			state.task_update_senders.write().await.insert(task_id, update_tx.clone());

			let task = match Task::new(
				input_file_path,
				task_id,
				&config_lock.outputs_dir,
				config_lock.ffmpeg_executable.clone(),
				update_tx.clone(),
			)
			.await
			{
				Ok(x) => x,
				Err(err) => {
					let err_string = err.to_string();
					tracing::error!("Failed to start task: {}", err_string);
					return EndpointResult::Err(
						StatusCode::INTERNAL_SERVER_ERROR,
						Some(err_string.into()),
					);
				}
			};

			state.tasks.write().await.insert(task_id, task);

			EndpointResult::Ok(task_id_string)
		}
		Err(err) => {
			EndpointResult::Err(StatusCode::BAD_REQUEST, Some(err.to_string().into()))
		}
	}
}

async fn status(state: State<AppState>) -> impl IntoResponse {
	format!("{:#?}", state.tasks.read().await)
}

#[derive(serde::Deserialize)]
struct StatusWsQuery {
	t: TaskId,
}

#[debug_handler]
async fn status_ws(
	ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
	state: State<AppState>,
	Query(StatusWsQuery { t }): Query<StatusWsQuery>,
) -> impl IntoResponse {
	let ws = match ws {
		Ok(x) => x,
		Err(err) => {
			tracing::info!("ws upgrade rejected: {}", err);
			return err.into_response();
		}
	};
	ws.on_failed_upgrade(|_| tracing::info!("ws upgrade failed"))
		.on_upgrade(move |ws| ws_handler(ws, state, t))
}

async fn ws_handler(mut ws: WebSocket, state: State<AppState>, target_task: TaskId) {
	tracing::info!("ws connected");
	if ws.send(ws::Message::Ping(Vec::new())).await.is_err() {
		tracing::info!("can't ping ws");
		return;
	}
	let Some(mut rx) =
		state.task_update_senders.read().await.get(&target_task).map(|x| x.subscribe())
	else {
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
			tokio::sync::watch::Receiver::
			let message = match msg.1 {
				Ok(status) => status.to_string(),
				Err(error) => format!("Error({error})"),
			};
			let Ok(_) = ws.send(ws::Message::Text(message)).await else {
				tracing::info!("failed to send ws message");
				return;
			};
		}
	});
}

#[derive(serde::Deserialize)]
struct DownloadQuery {
	file: String,
}

#[debug_handler]
async fn download(query: Query<DownloadQuery>) -> impl IntoResponse {
	// let filtered_file =
	// 	query.file.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>();

	// let filename =
	// 	CONFIG.read().await.temp_dir_root.join(filtered_file).join("output.mp4");
	// tracing::debug!("try download filename: {}", filename.display());

	// let Ok(file) = tokio::fs::File::open(filename).await else {
	// 	return Err((StatusCode::NOT_FOUND, "Not found"));
	// };

	// let file_len = file.metadata().await.unwrap().len();
	// let stream = ReaderStream::new(file);

	// let body = StreamBody::new(stream);

	// let mut headers = HeaderMap::new();
	// headers.append(CONTENT_TYPE, "video/mp4".parse().unwrap());
	// headers.append(
	// 	CONTENT_DISPOSITION,
	// 	"attachment; filename=\"test.mp4\"".parse().unwrap(),
	// );
	// headers.append(CONTENT_LENGTH, file_len.into());

	// Ok((StatusCode::OK, headers, body))
	todo!()
}
