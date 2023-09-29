use std::{
	ffi::OsStr,
	fmt::Display,
	path::{Path, PathBuf},
	process::Stdio,
	sync::{atomic::Ordering, Arc},
};

use atomic_float::AtomicF32;
use rand::Rng;
use tokio::{
	io::{self, AsyncBufReadExt, AsyncReadExt, BufReader},
	process::{Child, Command},
	sync::{Mutex, Notify, RwLock},
};

use crate::{
	config::CONFIG,
	ffmpeg::{FFmpeg, FFmpegError},
};

pub type TaskUpdateSender = tokio::sync::broadcast::Sender<TaskUpdateMessage>;
/// Represents a task that is currently running
/// with a handle to the encoder process
///
/// `dir_name` is the name of the directory where
/// the task's files are stored.
#[derive(Debug)]
pub struct Task {
	tokio_handle: tokio::task::JoinHandle<()>,
	id: TaskId,
	last_status: Arc<Mutex<TaskStatus>>,
	task_update_tx: TaskUpdateSender,
}

#[derive(Clone, Debug)]
pub enum TaskStatus {
	InProgress(f32),
	Error(FFmpegError),
	Completed,
}

impl Display for TaskStatus {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::InProgress(x) => write!(f, "InProgress({x})"),
			Self::Error(x) => write!(f, "Error({x})"),
			Self::Completed => write!(f, "Completed"),
		}
	}
}

/// id of task
/// also used as name of task's input and output files
pub type TaskId = u64;
pub type TaskUpdateMessage = (TaskId, Result<TaskStatus, FFmpegError>);

macro_rules! try_else {
	($expr:expr, $vn:ident, $div:block) => {
		match $expr {
			Result::Ok(val) => val,
			Result::Err($vn) => $div,
		}
	};
}

impl Task {
	/// initialize and start a new task
	/// also start a tokio task
	/// to observe it
	pub async fn new(
		input_file: PathBuf,
		task_id: TaskId,
		outputs_dir: &Path,
		ffmpeg_executable: PathBuf,
	) -> io::Result<Task> {
		let output_file = outputs_dir.join(format!("{task_id}"));

		let last_status = Arc::new(Mutex::new(TaskStatus::InProgress(0.0)));
		let task_update_tx = tokio::sync::broadcast::Sender::new(8);

		let tokio_handle = tokio::task::spawn({
			let last_status = last_status.clone();
			let task_update_tx = task_update_tx.clone();
			async move {
				let conversion_result = Self::run_conversion(
					input_file,
					output_file,
					ffmpeg_executable,
					last_status.clone(),
					task_update_tx.clone(),
					task_id,
				)
				.await;

				// ignore send result
				let _ = match conversion_result {
					Ok(_) => {
						*last_status.lock().await = TaskStatus::Completed;
						task_update_tx.send((task_id, Ok(TaskStatus::Completed)))
					}
					Err(msg) => {
						*last_status.lock().await = TaskStatus::Error(msg.clone());
						task_update_tx.send((task_id, Err(msg)))
					}
				};
			}
		});

		Ok(Self {
			tokio_handle,
			id: task_id,
			last_status,
			task_update_tx,
		})
	}

	pub async fn cancel(self) {
		// self.tokio_handle.abort();
		// let mut handle = self.handle.write().await;
		// handle.kill().await.unwrap();

		todo!()
	}

	pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<TaskUpdateMessage> {
		self.task_update_tx.subscribe()
	}

	pub async fn last_status(&self) -> TaskStatus {
		self.last_status.lock().await.clone()
	}

	pub fn gen_id() -> TaskId {
		rand::thread_rng().gen()
	}

	async fn run_conversion(
		input_file: PathBuf,
		output_file: PathBuf,
		ffmpeg_executable: PathBuf,
		status: Arc<Mutex<TaskStatus>>,
		progress_tx: tokio::sync::broadcast::Sender<TaskUpdateMessage>,
		task_id: TaskId,
	) -> Result<(), FFmpegError> {
		tracing::debug!("begin task");

		let ffmpeg = FFmpeg::new(input_file.to_path_buf(), output_file, ffmpeg_executable.to_path_buf());

		let analysis = try_else!(ffmpeg.analyze_silence().await, err, {
			tracing::info!("analyze silence error: {:?}", err);
			return Err(err);
		});

		let mut child = try_else!(ffmpeg.spawn_remove_silence(&analysis.audible), err, {
			tracing::info!("remove silence error: {:?}", err);
			return Err(FFmpegError::IO(err.into()));
		});

		let stdout = child.stdout.take().unwrap();
		let mut lines = BufReader::new(stdout).lines();

		let stderr = child.stderr.take().unwrap();
		let mut err_lines = BufReader::new(stderr).lines();
		// loop awaits on ffmpeg's stdout
		loop {
			// race reading an error and reading a line
			// break if EOF
			let line = tokio::select! {
				x = err_lines.next_line() => {
					tracing::info!("error");
					match x.unwrap() {
						Some(x) => return Err(FFmpegError::FFmpeg(format!("{:?}", x))),
						None => None
					}

				}
				x = lines.next_line() => x.unwrap()
			};

			let Some(line) = line else {
				break;
			};

			// output_time_ms is the same as _us bug in ffmpeg
			if line.starts_with("out_time_us=") {
				let current_time_us = line.split_at(12).1.parse::<f32>().unwrap();
				let current_progress = current_time_us / analysis.duration.as_micros() as f32;

				tracing::debug!(
					"progress: {} | {} {} {:?}",
					current_progress,
					current_time_us,
					analysis.duration.as_micros(),
					line
				);
				*status.lock().await = TaskStatus::InProgress(current_progress);

				// fails if no websockets are listening, which is fine
				let _ = progress_tx.send((task_id, Ok(TaskStatus::InProgress(current_progress))));
				if current_progress >= 1.0 || matches!(child.try_wait(), Ok(Some(_))) {
					break;
				}
			}
		}

		let status = child.wait().await.unwrap();
		tracing::debug!("status: {:?} success: {}", status, status.success());

		tracing::debug!("end task");

		Ok(())
	}
}
