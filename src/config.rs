use std::{
	ops::Deref,
	path::{Path, PathBuf},
	sync::{OnceLock, RwLock, RwLockReadGuard},
};

const CONFIG_PATH: &str = "config.toml";

/// Returned by [`reload_config`].
/// [`ConfigReloadResult::Ok`] - Config has been updated or written or initialized
///
/// [`ConfigReloadResult::Err`] - Some error was encountered. This
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigReloadResult {
	Ok,
	Err,
}

/// Wrapper for [`tracing::Level`] which supports serde
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum LogLevel {
	Trace,
	Debug,
	Info,
	Warn,
	Error,
}

impl From<LogLevel> for tracing::Level {
	fn from(value: LogLevel) -> Self {
		match value {
			LogLevel::Trace => tracing::Level::TRACE,
			LogLevel::Debug => tracing::Level::DEBUG,
			LogLevel::Info => tracing::Level::INFO,
			LogLevel::Warn => tracing::Level::WARN,
			LogLevel::Error => tracing::Level::ERROR,
		}
	}
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
	pub log_level: LogLevel,
	pub temp_dir_root: PathBuf,
	pub log_file_root: PathBuf,
	pub ffmpeg_executable: PathBuf,
	pub ffprobe_executable: PathBuf,
	pub max_file_size: u64,
	pub port: u16,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			#[cfg(debug_assertions)]
			log_level: LogLevel::Trace,
			#[cfg(not(debug_assertions))]
			log_level: LogLevel::Info,
			temp_dir_root: PathBuf::from("./temp"),
			log_file_root: PathBuf::from("./logs"),
			ffmpeg_executable: PathBuf::from("ffmpeg"),
			ffprobe_executable: PathBuf::from("ffprobe"),
			max_file_size: 1024 * 1024 * 1024, // 1 GiB
			port: 3000,
		}
	}
}

impl Config {
	pub fn ffmpeg_found(&self) -> bool {
		which::which(&self.ffmpeg_executable).is_ok()
	}

	pub fn ffprobe_found(&self) -> bool {
		which::which(&self.ffprobe_executable).is_ok()
	}

	pub fn init_temp_dir(&self) -> std::io::Result<()> {
		std::fs::create_dir_all(&self.temp_dir_root)
	}

	pub fn init_log_file_dir(&self) -> std::io::Result<()> {
		std::fs::create_dir_all(&self.log_file_root)
	}
}

pub struct ConfigStatic(OnceLock<RwLock<Config>>);

impl ConfigStatic {
	pub fn read(&self) -> RwLockReadGuard<'_, Config> {
		// SAFETY: config is intialized in main
		unsafe { self.0.get().unwrap_unchecked().read().unwrap() }
	}
}

pub static CONFIG: ConfigStatic = ConfigStatic(OnceLock::new());

/// Reloads the app config.
///
/// Loads the config form the file or writes to the file, if one was created by calling this function.
/// Initializes the app config if it has not been initialized yet.
/// No-op if the app config has not been initialized yet.
pub fn reload_config() -> ConfigReloadResult {
	let config_file_path = Path::new(CONFIG_PATH);

	let mut current_config_lock = CONFIG
		.0
		.get_or_init(|| RwLock::new(Config::default()))
		.write()
		.expect("config lock is poisoned");

	// Read the config file if it exists.
	// Returns on io or parse error, otherwise evaluates to [`Option<Config>`],
	// depending on whether config file exists
	let config_read_option: Option<Config> = match config_file_path.try_exists() {
		Ok(false) => None,
		Ok(true) => {
			let Ok(file_data) = std::fs::read_to_string(config_file_path).map_err(|_| ())
			else {
				return ConfigReloadResult::Err;
			};
			toml::from_str(&file_data).ok()
		}
		Err(_) => return ConfigReloadResult::Err,
	};

	match config_read_option {
		// Config file exists => replace current config with the loaded one
		Some(new_config) => {
			*current_config_lock = new_config;
		}
		// Config file does not exist => write current config to file
		None => {
			let config_string = toml::to_string(&*current_config_lock)
				.expect("failed to serialize config to toml");
			// this should never fail, because we already made some syscalls to check if the file exists
			std::fs::write(config_file_path, config_string)
				.expect("failed to write config file");
		}
	}

	// disallow temp dir in current path for security
	// since it's accessed through /download route
	assert!(
		current_config_lock.temp_dir_root != Path::new(".\\")
			&& current_config_lock.temp_dir_root != Path::new("./")
			&& current_config_lock.temp_dir_root != Path::new(".")
			&& current_config_lock.temp_dir_root != Path::new("")
	);

	current_config_lock.init_log_file_dir().expect("failed to create log file dir");
	current_config_lock.init_temp_dir().expect("failed to create temp dir");

	ConfigReloadResult::Ok
}
