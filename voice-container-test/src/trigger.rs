use serde::{Deserialize, Serialize};

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
