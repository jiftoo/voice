use std::{
	cell::Cell,
	ffi::OsStr,
	fmt::Display,
	mem::MaybeUninit,
	path::{Path, PathBuf},
	process::{ExitStatus, Stdio},
	sync::{atomic::Ordering, Arc},
};

use atomic_float::AtomicF32;
use rand::Rng;
use tokio::{
	io::{self, AsyncBufReadExt, AsyncReadExt, BufReader},
	process::{Child, Command},
	sync::{Mutex, Notify, RwLock},
	task::block_in_place,
};

use crate::{
	config::CONFIG,
	ffmpeg::{FFmpeg, FFmpegError},
};

pub type TaskUpdateSender = tokio::sync::broadcast::Sender<TaskUpdateMessage>;

#[derive(Debug)]
struct InnerTask {
	last_status: TaskStatus,
	task_update_tx: TaskUpdateSender,
}

/// Represents a task that is currently running
/// with a handle to the encoder process
///
/// `dir_name` is the name of the directory where
/// the task's files are stored.
#[derive(Debug)]
pub struct Task {
	id: TaskId,
	tokio_handle: tokio::task::JoinHandle<()>,
	inner: Arc<RwLock<InnerTask>>,
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
pub type TaskUpdateMessage = (TaskId, TaskStatus);

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
	pub fn new(
		input_file: PathBuf,
		task_id: TaskId,
		outputs_dir: &Path,
		ffmpeg_executable: PathBuf,
	) -> io::Result<Task> {
		let output_file = outputs_dir.join(format!("{task_id}"));

		let last_status = TaskStatus::InProgress(0.0);
		let task_update_tx = tokio::sync::broadcast::Sender::new(8);

		let inner_task = Arc::new(RwLock::new(InnerTask {
			last_status,
			task_update_tx: task_update_tx.clone(),
		}));

		let tokio_handle = tokio::task::spawn({
			let inner_task = inner_task.clone();
			async move {
				let conversion_result =
					Self::run_conversion(input_file, output_file, ffmpeg_executable, inner_task.clone(), task_id).await;

				// ignore send result
				let final_status = match conversion_result {
					Ok(_) => TaskStatus::Completed,
					Err(msg) => TaskStatus::Error(msg),
				};

				Self::update_status(&mut *inner_task.write().await, task_id, final_status).await;
			}
		});

		Ok(Self {
			tokio_handle,
			id: task_id,
			inner: inner_task,
		})
	}

	pub async fn cancel(self) {
		self.tokio_handle.abort();
		let mut this = self.inner.write().await;
		Self::update_status(&mut this, self.id, TaskStatus::Completed).await;
	}

	pub async fn subscribe(&self) -> tokio::sync::broadcast::Receiver<TaskUpdateMessage> {
		self.inner.read().await.task_update_tx.subscribe()
	}

	pub async fn last_status(&self) -> TaskStatus {
		self.inner.read().await.last_status.clone()
	}

	pub fn gen_id() -> TaskId {
		rand::thread_rng().gen()
	}

	async fn update_status(inner: &mut InnerTask, id: TaskId, new_status: TaskStatus) {
		inner.last_status = new_status.clone();
		let _ = inner.task_update_tx.send((id, new_status.clone()));
	}

	async fn run_conversion(
		input_file: PathBuf,
		output_file: PathBuf,
		ffmpeg_executable: PathBuf,
		inner: Arc<RwLock<InnerTask>>,
		task_id: TaskId,
	) -> Result<(), FFmpegError> {
		tracing::debug!("begin task");

		let ffmpeg = FFmpeg::new(input_file.to_path_buf(), output_file, ffmpeg_executable.to_path_buf());

		let analysis = try_else!(ffmpeg.analyze_silence().await, err, {
			tracing::info!("analyze silence error: {:?}", err);
			return Err(err);
		});

		let mut child = try_else!(ffmpeg.spawn_remove_silence(&analysis.audible).await, err, {
			tracing::info!("remove silence error: {:?}", err);
			return Err(FFmpegError::IO(err.into()));
		});

		let stdout = child.stdout.take().unwrap();
		let mut lines = BufReader::new(stdout).lines();

		let stderr = child.stderr.take().unwrap();
		let mut err_lines = BufReader::new(stderr).lines();
		let mut error_log = Vec::new();
		// loop awaits on ffmpeg's stdout
		loop {
			// race reading an error and reading a line
			// break if EOF
			let line = tokio::select! {
				x = err_lines.next_line() => {
					match x.unwrap() {
						Some(x) => {
							error_log.push(x);
							None
						},
						None => None
					}

				}
				x = lines.next_line() => x.unwrap()
			};

			// abort if the process is done
			match child.try_wait() {
				Ok(None) => {}
				Ok(Some(status)) => {
					if status.success() {
						break;
					} else {
						return Err(FFmpegError::FFmpeg(error_log.join("\n")));
					}
				}
				err @ Err(_) => {
					tracing::warn!("error while calling try_wait()");
					err.unwrap();
				}
			}

			// skip iteration if the line is from stderr
			let Some(line) = line else {
				continue;
			};

			// output_time_ms is the same as _us bug in ffmpeg
			if line.starts_with("out_time_us=") {
				let mut current_time_us = line.split_at(12).1.parse::<i64>().unwrap();
				if current_time_us == -9223372036854775807 {
					current_time_us = 0;
				}
				let current_progress = current_time_us as f32 / analysis.duration.as_micros() as f32;

				tracing::debug!(
					"progress: {} | {} {} {:?}",
					current_progress,
					current_time_us,
					analysis.duration.as_micros(),
					line
				);

				Self::update_status(&mut *inner.write().await, task_id, TaskStatus::InProgress(current_progress)).await;
			}
		}

		let status = child.wait().await.unwrap();
		tracing::debug!("status: {:?} success: {}", status, status.success());

		if !error_log.is_empty() {
			tracing::info!("errors during task: {}", error_log.join("\n"));
		}

		tracing::debug!("end task");

		Ok(())
	}
}
