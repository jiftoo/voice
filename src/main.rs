use std::time::SystemTime;

use tracing_subscriber::{filter::LevelFilter, prelude::__tracing_subscriber_SubscriberExt, EnvFilter, Layer};

pub mod avg;
pub mod config;
pub mod ffmpeg;
mod task;
pub mod web;

#[tokio::main]
async fn main() {
	/*
	1. parse the config file or create a default one
	2. check if ffmpeg and ffprobe are present, abort if not
	3. set up logging to stdout and a log file, abort if file permissions denied
	4. create the task manager thread
	5. spin up the server, bind to port in the config and abort if it fails
	6. create the stdin listener thread, do nothing if not tty
	*/

	// TODO: Better ffmpeg error reporing; maybe a separate ffmpeg log file
	// TODO: Clean up after panicked/failed tasks
	// TODO: Implement re-encoding, since browsers don't like concatenated mp4.
	// TODO: The rest of the frontend and API
	// TODO: check if the file is suitable before processing

	// println until logger is set up
	println!("{} v{} by {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_AUTHORS"));
	println!(";D");

	// initialize config
	config::reload_config().await;

	// block to drop config_lock
	{
		let config_lock = config::CONFIG.read().await;

		println!("config: {:#?}", config_lock);

		if !config_lock.encoder_found() {
			println!("error: encoder not found. specified path: \"{}\"", config_lock.ffmpeg_executable.display());
			std::process::exit(1);
		}

		// initialize logging

		let log_file_name = "log.txt";
		let stdout_subscriber = tracing_subscriber::fmt::layer()
			// .pretty()
			.with_writer(std::io::stdout)
			.with_filter(
				EnvFilter::from_default_env()
					.add_directive(LevelFilter::from_level(config_lock.log_level.into()).into())
					.add_directive("hyper::proto::h1::io=info".parse().unwrap())
					.add_directive("hyper::proto::h1::conn=info".parse().unwrap())
					.add_directive("hyper::proto::h1::decode=info".parse().unwrap())
					.add_directive("hyper::proto::h1::encode=info".parse().unwrap())
					.add_directive("hyper::proto::h1::role=info".parse().unwrap()),
			);
		let file_subscriber = tracing_subscriber::fmt::layer()
			// .pretty()
			.with_writer(tracing_appender::rolling::never(&config_lock.log_file_root, log_file_name))
			.with_ansi(false)
			.with_filter(tracing_subscriber::filter::LevelFilter::from_level(config_lock.log_level.into()))
			.with_filter(
				EnvFilter::from_default_env()
					.add_directive("hyper::proto::h1::io=info".parse().unwrap())
					.add_directive("hyper::proto::h1::conn=info".parse().unwrap())
					.add_directive("hyper::proto::h1::decode=info".parse().unwrap())
					.add_directive("hyper::proto::h1::encode=info".parse().unwrap())
					.add_directive("hyper::proto::h1::role=info".parse().unwrap()),
			);
		tracing::subscriber::set_global_default(
			tracing_subscriber::registry()
				.with(stdout_subscriber)
				.with(file_subscriber),
		)
		.expect("failed to set global default subscriber");

		tracing::info!("=========================================================================");
		tracing::info!("Logging initialized");
		tracing::info!("Log level: {:?}", config_lock.log_level);
		tracing::info!("Writing to file: {}", config_lock.log_file_root.join(log_file_name).display());
	}

	web::initialize_server().await;
}
