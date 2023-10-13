use std::{
	path::{Path, PathBuf},
	sync::OnceLock,
};
use tokio::sync::RwLock;

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
	/// level to log at
	pub log_level: LogLevel,
	/// incoming file storage
	pub inputs_dir: PathBuf,
	/// encoding result storage
	pub outputs_dir: PathBuf,
	/// log file dir
	pub log_file_root: PathBuf,
	/// encoder executable path
	pub ffmpeg_executable: PathBuf,
	/// web file root
	pub web_root: PathBuf,
	/// max input file size in bytes
	pub max_file_size: u64,
	/// port to bind to
	pub port: u16,
	/// delete input/output files after this many minutes
	pub delete_files_after_minutes: u64,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			#[cfg(debug_assertions)]
			log_level: LogLevel::Debug,
			#[cfg(not(debug_assertions))]
			log_level: LogLevel::Info,
			inputs_dir: PathBuf::from("./inputs"),
			outputs_dir: PathBuf::from("./outputs"),
			log_file_root: PathBuf::from("./logs"),
			web_root: PathBuf::from("./web"),
			ffmpeg_executable: PathBuf::from("ffmpeg"),
			max_file_size: 1024 * 1024 * 1024, // 1 GiB
			port: 80,
			delete_files_after_minutes: 60,
		}
	}
}

impl Config {
	pub fn encoder_found(&self) -> bool {
		which::which(&self.ffmpeg_executable).is_ok()
	}

	pub fn web_dir_found(&self) -> bool {
		if !self.web_root.exists() {
			return false;
		}

		let mut index = false;
		let mut completed = false;
		for file in self
			.web_root
			.read_dir()
			.unwrap()
			.flatten()
			.map(|x| x.file_name().to_string_lossy().to_string())
		{
			if file == "index.html" {
				index = true;
			}
			if file == "completed.html" {
				completed = true;
			}
		}

		index && completed
	}

	pub fn init_directories(&self) -> std::io::Result<()> {
		std::fs::create_dir_all(&self.inputs_dir)?;
		std::fs::create_dir_all(&self.outputs_dir)?;
		std::fs::create_dir_all(&self.log_file_root)
	}
}

pub struct ConfigStatic(OnceLock<RwLock<Config>>);

impl ConfigStatic {
	pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, Config> {
		// SAFETY: config is intialized in main
		unsafe { self.0.get().unwrap_unchecked().read().await }
	}
}

pub static CONFIG: ConfigStatic = ConfigStatic(OnceLock::new());

/// Reloads the app config.
///
/// Loads the config form the file or writes to the file, if one was created by calling this function.
/// Initializes the app config if it has not been initialized yet.
/// No-op if the app config has not been initialized yet.
pub async fn reload_config() -> ConfigReloadResult {
	let config_file_path = Path::new(CONFIG_PATH);

	let mut current_config_lock =
		CONFIG.0.get_or_init(|| RwLock::new(Config::default())).write().await;

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

	current_config_lock
		.init_directories()
		.expect("failed to create necessary directories");

	ConfigReloadResult::Ok
}
