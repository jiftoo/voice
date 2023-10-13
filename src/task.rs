use std::{
	fmt::Display,
	path::{Path, PathBuf},
	sync::Arc,
	time::Duration,
};

use rand::Rng;
use tokio::{
	io::{self, AsyncBufReadExt, AsyncReadExt, BufReader},
	sync::RwLock,
};

use crate::ffmpeg::{FFmpeg, FFmpegError};

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

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type", content = "data")]
pub enum TaskStatus {
	InProgress {
		progress: f32,
		speed: f32,
	},
	#[serde(serialize_with = "display_serialize")]
	Error(FFmpegError),
	Completed,
}

fn display_serialize<S: serde::Serializer, T: Display>(
	x: &T,
	s: S,
) -> Result<S::Ok, S::Error> {
	s.collect_str(x)
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

enum StatsParse {
	Time(Duration),
	Speed(f32),
	/// error or not any other stat
	Other,
}

impl StatsParse {
	fn parse_line(line: &str) -> Self {
		let mut line = line.split('=');

		let Some(lhs) = line.next() else {
			return Self::Other;
		};
		let Some(rhs) = line.next() else {
			return Self::Other;
		};

		match lhs {
			"out_time_ms" => rhs
				.parse::<i64>()
				.map(|x| {
					// time is negative during first frame
					Self::Time(Duration::from_micros(x.max(0) as u64))
				})
				.unwrap_or(Self::Other),
			"speed" => rhs
				.trim_end_matches('x')
				.parse::<f32>()
				.map(Self::Speed)
				.unwrap_or(Self::Other),
			_ => Self::Other,
		}
	}
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

		let last_status = TaskStatus::InProgress { progress: 0.0, speed: 0.0 };
		let task_update_tx = tokio::sync::broadcast::Sender::new(8);

		let inner_task = Arc::new(RwLock::new(InnerTask {
			last_status,
			task_update_tx: task_update_tx.clone(),
		}));

		let tokio_handle = tokio::task::spawn({
			let inner_task = inner_task.clone();
			async move {
				let conversion_result = Self::run_conversion(
					input_file,
					output_file,
					ffmpeg_executable,
					inner_task.clone(),
					task_id,
				)
				.await;

				// ignore send result
				let final_status = match conversion_result {
					Ok(_) => TaskStatus::Completed,
					Err(msg) => TaskStatus::Error(msg),
				};

				tracing::debug!("sent last update: {final_status:?}");
				Self::update_status(
					&mut *inner_task.write().await,
					task_id,
					final_status,
				)
				.await;
			}
		});

		Ok(Self { tokio_handle, id: task_id, inner: inner_task })
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
		let _res = inner.task_update_tx.send((id, new_status.clone()));
	}

	async fn run_conversion(
		input_file: PathBuf,
		output_file: PathBuf,
		ffmpeg_executable: PathBuf,
		inner: Arc<RwLock<InnerTask>>,
		task_id: TaskId,
	) -> Result<(), FFmpegError> {
		tracing::debug!("begin task");

		let ffmpeg = FFmpeg::new(
			input_file.to_path_buf(),
			output_file,
			ffmpeg_executable.to_path_buf(),
		);

		let analysis = try_else!(ffmpeg.analyze_silence().await, err, {
			tracing::info!("analyze silence error: {:?}", err);
			return Err(err);
		});

		let playtime_after_conversion_s =
			analysis.audible.iter().map(|x| x.end - x.start).sum::<f32>();

		tracing::debug!(
			"total playtime: {}s; playtime after conversion: {}s; playtime reduced by {}%",
			analysis.duration.as_secs_f32(),
			playtime_after_conversion_s,
			(1.0 - playtime_after_conversion_s / analysis.duration.as_secs_f32()) * 100.0
		);

		let mut child =
			try_else!(ffmpeg.spawn_remove_silence(&analysis.audible).await, err, {
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
				_ = tokio::time::sleep(Duration::from_secs(1)) => {
					tracing::warn!("ffmpeg lagging");
					None
				}
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
				Ok(Some(_)) => {
					break;
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

			let mut inner_lock = inner.write().await;

			// output_time_ms is the same as _us bug in ffmpeg
			if let TaskStatus::InProgress { ref mut progress, ref mut speed } =
				inner_lock.last_status
			{
				match StatsParse::parse_line(&line) {
					StatsParse::Speed(new_speed) => {
						*speed = new_speed;
					}
					StatsParse::Time(new_time) => {
						let new_progress =
							new_time.as_secs_f32() / (playtime_after_conversion_s);
						*progress = new_progress;
					}
					_ => {
						// nothing was updated
						continue;
					}
				}
			}
			let last_status = inner_lock.last_status.clone();
			Self::update_status(&mut inner_lock, task_id, last_status).await;
			tracing::debug!("status: {:?}", inner_lock.last_status);
		}

		let status = child.wait().await.unwrap();
		tracing::debug!("status: {:?} success: {}", status, status.success());

		if status.success() {
			Ok(())
		} else {
			Err(FFmpegError::FFmpeg(error_log.join("\n")))
		}

		// if !error_log.is_empty() {
		// 	tracing::info!("errors during task: {}", error_log.join("\n"));
		// }
	}
}
