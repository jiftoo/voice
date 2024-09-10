use std::{str::FromStr, sync::Arc};

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

use crate::{RemoteFileIdentifier, RemoteFileKind, EMPTY_REMOTE_FILE_IDENTIFIER};

#[derive(Debug, Serialize, Deserialize)]
pub struct TriggerData {
	/// related to batching. as long as the group size is 1, this is always a tuple of size 1
	pub messages: [Message; 1],
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
	pub event_metadata: EventMetadata,
	pub details: Details,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventMetadata {
	pub event_id: String,
	pub event_type: String,
	pub created_at: String,
	pub tracing_context: TracingContext,
	pub cloud_id: String,
	pub folder_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TracingContext {
	pub trace_id: String,
	pub span_id: String,
	pub parent_span_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Details {
	pub bucket_id: String,
	pub object_id: String,
}

pub fn trigger_listener(on_trigger: impl Fn(RemoteFileIdentifier) + Send + Sync + 'static) -> Router {
	Router::new().route("/", post(trigger)).with_state(Arc::new(on_trigger))
}

async fn trigger(
	State(on_trigger): State<Arc<impl Fn(RemoteFileIdentifier) + Send + Sync>>,
	Json(TriggerData { messages: [Message { details, event_metadata }] }): Json<TriggerData>,
) -> StatusCode {
	println!("handling trigger: {}, {}", event_metadata.event_id, event_metadata.event_type);

	let split: Vec<&str> = details.object_id.split('/').collect();
	if split.len() != 2 {
		eprintln!("object_id malformed: {}", details.object_id);
		return StatusCode::BAD_REQUEST;
	}

	let Ok(file_identifier) = RemoteFileIdentifier::from_str(split[0]) else {
		eprintln!("object_id malformed (expected valid remote file identifier): {:?}", details.object_id);
		return StatusCode::BAD_REQUEST;
	};

	let remote_file = RemoteFileKind::VideoInput(file_identifier);
	let dir_name = remote_file.as_dir_name();
	if split[1] != dir_name {
		eprintln!("object_id malformed (expected video input `{dir_name}`): {}", details.object_id);
		return StatusCode::BAD_REQUEST;
	}

	on_trigger(file_identifier);

	StatusCode::OK
}
