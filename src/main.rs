use std::time::SystemTime;

use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, Layer};

pub mod avg;
pub mod config;
pub mod ffmpeg;
pub mod task;
pub mod video;

fn main() {
	/*
	1. parse the config file or create a default one
	2. check if ffmpeg and ffprobe are present, abort if not
	3. set up logging to stdout and a log file, abort if file permissions denied
	4. create the task manager thread
	5. spin up the server, bind to port in the config and abort if it fails
	6. create the stdin listener thread, do nothing if not tty
	*/

	// println until logger is set up
	println!(
		"{} v{} by {}",
		env!("CARGO_PKG_NAME"),
		env!("CARGO_PKG_VERSION"),
		env!("CARGO_PKG_AUTHORS")
	);
	println!(";D");

	// initialize config
	config::reload_config();

	let config_lock = config::CONFIG.read();

	println!("config: {:#?}", config_lock);

	// checks if both ffmpeg and ffprobe are present
	// otherwise exist with an error message
	([
		("ffmpeg", config_lock.ffmpeg_found(), &config_lock.ffmpeg_executable),
		("ffprobe", config_lock.ffprobe_found(), &config_lock.ffprobe_executable),
	]
	.into_iter()
	.filter(|(_, found, _)| !found)
	.map(|(name, _, path)| {
		println!("error: {name} not found. specified path: \"{}\"", path.display());
	})
	.count() != 0)
		.then(|| {
			// It's 1:42 therefore I must write arcane one-liners instead of using a for loop
			std::process::exit(1);
		});

	// initialize logging

	// let log_file_name = format!(
	// 	"{}.log",
	// 	SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
	// );
	let log_file_name = "log.txt";
	let stdout_subscriber = tracing_subscriber::fmt::layer()
		// .pretty()
		.with_writer(std::io::stdout)
		.with_filter(tracing_subscriber::filter::LevelFilter::from_level(
			config_lock.log_level.into(),
		));
	let file_subscriber = tracing_subscriber::fmt::layer()
		// .pretty()
		.with_writer(tracing_appender::rolling::never(
			&config_lock.log_file_root,
			&log_file_name,
		))
		.with_ansi(false)
		.with_filter(tracing_subscriber::filter::LevelFilter::from_level(
			config_lock.log_level.into(),
		));
	tracing::subscriber::set_global_default(
		tracing_subscriber::registry().with(stdout_subscriber).with(file_subscriber),
	)
	.expect("failed to set global default subscriber");

	tracing::info!(
		"========================================================================="
	);
	tracing::info!("Logging initialized");
	tracing::info!("Log level: {:?}", config_lock.log_level);
	tracing::info!(
		"Writing to file: {}",
		config_lock.log_file_root.join(log_file_name).display()
	);

	// TODO: task manager thread and the rest...
}
