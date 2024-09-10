mod trigger;

use std::path::Path;

use axum::{http::StatusCode, routing::post, Json, Router};
use trigger::{Message, TriggerData};

#[tokio::main]
async fn main() {
	println!("Started");

	let router = Router::new().route("/", post(handle_trigger));
	// .route("/test", get(run_tests_upgrade))
	// .layer(axum::middleware::from_fn(|req: Request, next: Next| async {
	// 	let mut res = next.run(req).await;
	// 	res.headers_mut()
	// 		.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, "GET, POST".parse().unwrap());
	// 	res.headers_mut().insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
	// 	res
	// }));

	let port: u16 = dbg!(std::env::var("PORT").map(|x| x.parse().unwrap()).unwrap_or(80));
	let bind = format!("0.0.0.0:{port}");

	let listener = tokio::net::TcpListener::bind(bind).await.unwrap();
	axum::serve(listener, router).await.unwrap();
}

async fn handle_trigger(
	Json(TriggerData { messages: [Message { details, event_metadata }] }): Json<TriggerData>,
) -> StatusCode {
	println!("handling trigger: {}, {}", event_metadata.event_id, event_metadata.event_type);

	let is_dir = Path::new("/bucket").is_dir();
	if !is_dir {
		panic!("/bucket does not exist or isn't a directory!");
	}

	let bucket_files = std::fs::read_dir("/bucket")
		.unwrap()
		.flatten()
		.map(|x| x.path().to_string_lossy().into_owned())
		.collect::<Vec<String>>();

	println!("files in bucket: {bucket_files:?}");

	let filename = details.object_id.split('/').last().unwrap();

	let new_file = bucket_files.into_iter().find(|x| x.contains(filename));
	println!("new file: {new_file:?}; checked {}", details.object_id);

	if let Some(path) = new_file {
		std::fs::remove_file(path).unwrap();
	}

	StatusCode::OK
}

// async fn run_tests_upgrade(upgrade: WebSocketUpgrade) -> impl IntoResponse {
// 	upgrade
// 		// needs both set
// 		.max_frame_size(1024 * 1024 * 256)
// 		.max_message_size(1024 * 1024 * 256)
// 		.on_upgrade(run_tests)
// }

// async fn run_tests(mut socket: WebSocket) {
// 	println!("websocket updgrade successful");

// 	let small_dummy = generate_dummy(64);
// 	println!("Sending {} bytes", small_dummy.len());
// 	socket.send(Message::Binary(small_dummy.clone())).await.expect("small dummy to succeed");
// 	println!("Sent");

// 	println!("Receiving small dummy");
// 	let message =
// 		socket.recv().await.expect("stream to be open").expect("receive small dummy to succeed");
// 	let message = match message {
// 		Message::Binary(data) => data,
// 		_ => panic!("message is not binary data"),
// 	};
// 	assert_eq!(message, small_dummy);

// 	let large_dummy = generate_dummy(134217728); // 128 mib
// 	println!("Sending {} bytes", large_dummy.len());
// 	socket.send(Message::Binary(large_dummy.clone())).await.expect("large dummy to succeed");
// 	println!("Sent");

// 	println!("Receiving large dummy");
// 	let message =
// 		socket.recv().await.expect("stream to be open").expect("receive large dummy to succeed");
// 	let message = match message {
// 		Message::Binary(data) => data,
// 		_ => panic!("message is not binary data"),
// 	};
// 	assert_eq!(message, large_dummy);

// 	println!("Writing {} bytes to mounted bucket", small_dummy.len());
// 	tokio::fs::create_dir_all("./bucket/small_dummy")
// 		.await
// 		.expect("create dir all to succeed");
// 	tokio::fs::write("./bucket/small_dummy/data", &small_dummy)
// 		.await
// 		.expect("writing small dummy to succeed");
// 	println!("Wrote");

// 	tokio::fs::create_dir_all("./bucket/large_dummy")
// 		.await
// 		.expect("create dir all to succeed");
// 	println!("Writing {} bytes to mounted bucket", large_dummy.len());
// 	tokio::fs::write("./bucket/large_dummy/data", &large_dummy)
// 		.await
// 		.expect("writing large dummy to succeed");
// 	println!("Wrote");

// 	println!("Done.");
// }

// fn generate_dummy(size: usize) -> Vec<u8> {
// 	std::iter::repeat_with(rand::random).take(size).collect::<Vec<u8>>()
// }
