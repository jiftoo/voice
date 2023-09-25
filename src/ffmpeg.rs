use std::{
	fmt::{Debug, Display},
	io::{BufRead, BufReader, Read},
	path::Path,
	process::{Child, Stdio},
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc, Mutex,
	},
	thread::{self},
	time::Duration,
};

#[derive(Debug, Clone, Copy)]
pub struct VideoPeriod {
	pub from: f64,
	pub to: f64,
}

impl Display for VideoPeriod {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "({}, {})", self.from, self.to)
	}
}

impl VideoPeriod {
	pub fn new(from: f64, to: f64) -> Self {
		Self { from, to }
	}

	pub fn duration(&self) -> f64 {
		self.to - self.from
	}
}

#[derive(Debug, Clone)]
pub struct VideoPeriods {
	silent_periods: Vec<VideoPeriod>,
}

impl VideoPeriods {
	pub fn silent_periods(&self) -> impl Iterator<Item = &VideoPeriod> + '_ {
		self.silent_periods.iter()
	}

	pub fn audible_periods(&self) -> impl Iterator<Item = VideoPeriod> + '_ {
		self.silent_periods
			.windows(2)
			.map(|ab| VideoPeriod { from: ab[0].to, to: ab[1].from })
	}
}

#[derive(Debug, Clone, Copy)]
pub enum AbortReason {
	Killed,
	Error,
}

#[derive(Debug, Clone)]
pub enum FFMpegThreadStatus<T: Clone> {
	Running,
	Finished(T),
	Aborted(AbortReason),
}

pub trait FFMpegFuture<R: Clone> {
	fn poll(&mut self) -> FFMpegThreadStatus<R>;
	fn abort_blocking(self) -> FFMpegThreadStatus<R>;
	fn get_progress(&self) -> Arc<Mutex<f32>>;
	fn abort(self) -> Option<thread::JoinHandle<anyhow::Result<R, AbortReason>>>
	where
		Self: std::marker::Sized,
	{
		None
	}
}

pub struct FFMpegThreadFuture<R: Clone> {
	should_abort: Arc<AtomicBool>,
	thread_handle: thread::JoinHandle<anyhow::Result<R, AbortReason>>,
	thread_status: Arc<Mutex<Option<FFMpegThreadStatus<R>>>>,
	progress: Arc<Mutex<f32>>,
}

impl<R: Clone + Send + 'static> FFMpegThreadFuture<R> {
	pub fn new<P, F>(thread_fn: P) -> Self
	where
		P: FnOnce(Arc<AtomicBool>, Arc<Mutex<f32>>) -> F,
		F: FnOnce() -> anyhow::Result<R, AbortReason> + Send + 'static,
	{
		let thread_status = Arc::new(Mutex::new(Some(FFMpegThreadStatus::Running)));
		let should_abort = Arc::new(AtomicBool::new(false));
		let progress = Arc::new(Mutex::new(0.0));
		Self {
			thread_handle: thread::spawn({
				let action = thread_fn(should_abort.clone(), progress.clone());
				let thread_status = thread_status.clone();
				move || {
					let result = action();
					let mut lock = thread_status.lock().unwrap();
					*lock = Some(match result.clone() {
						Ok(result) => FFMpegThreadStatus::Finished(result),
						Err(reason) => FFMpegThreadStatus::Aborted(reason),
					});
					result
				}
			}),
			should_abort,
			thread_status,
			progress,
		}
	}
}

impl<R: Clone + Send + 'static> FFMpegFuture<R> for FFMpegThreadFuture<R> {
	fn poll(&mut self) -> FFMpegThreadStatus<R> {
		let lock = self.thread_status.clone();
		let lock = lock.lock().unwrap();
		lock.as_ref().cloned().unwrap()
	}

	fn abort_blocking(self) -> FFMpegThreadStatus<R> {
		self.should_abort.store(true, Ordering::Relaxed);
		let _ = self.thread_handle.join().unwrap();
		Arc::try_unwrap(self.thread_status)
			.unwrap_or_else(|_| panic!("Arc::try_unwrap"))
			.into_inner()
			.unwrap()
			.unwrap()
	}

	fn abort(self) -> Option<thread::JoinHandle<anyhow::Result<R, AbortReason>>> {
		self.should_abort.store(true, Ordering::Relaxed);
		Some(self.thread_handle)
	}

	fn get_progress(&self) -> Arc<Mutex<f32>> {
		self.progress.clone()
	}
}

pub struct FFMpegReadyFuture<R>(R, Arc<Mutex<f32>>);

impl<R> FFMpegReadyFuture<R> {
	pub fn new(v: R) -> Self {
		Self(v, Arc::new(Mutex::new(1.0)))
	}
}

impl<R: Clone> FFMpegFuture<R> for FFMpegReadyFuture<R> {
	fn poll(&mut self) -> FFMpegThreadStatus<R> {
		FFMpegThreadStatus::Finished(self.0.clone())
	}
	fn abort_blocking(mut self) -> FFMpegThreadStatus<R> {
		self.poll()
	}
	fn get_progress(&self) -> Arc<Mutex<f32>> {
		self.1.clone()
	}
}

struct TrackProgress {
	progress: Arc<Mutex<f32>>,
	total_frames: usize,
}

fn read_or_terminate(
	read: &mut impl Read,
	should_stop: &AtomicBool,
	track_progress: Option<TrackProgress>,
) -> Option<String> {
	let mut result = Vec::new();
	let mut line = String::new();
	let mut buf_reader = BufReader::new(read);
	loop {
		if should_stop.load(Ordering::Relaxed) {
			return None;
		}
		if buf_reader.read_line(&mut line).unwrap() == 0 {
			return Some(result.join(""));
		}

		result.push(line.clone());
		if let Some(x) = &track_progress {
			if line.starts_with("frame") {
				let frame_n: f32 = line
					.split_ascii_whitespace()
					.next()
					.unwrap()
					.split_once(':')
					.unwrap()
					.1
					.trim()
					.parse()
					.unwrap();
				*x.progress.lock().unwrap() = frame_n / x.total_frames as f32;
			}
		}
		line.clear();
	}
}

fn spawn_ffmpeg(args: impl AsRef<str>, stdout: Stdio, stderr: Stdio) -> Child {
	let mut proc = std::process::Command::new("ffmpeg");
	// println!("spawn ffmpeg: {}", args.as_ref());
	proc.args(args.as_ref().split_ascii_whitespace())
		.stdout(stdout)
		.stderr(stderr)
		.stdin(Stdio::null())
		.spawn()
		.unwrap()
}

fn spawn_ffprobe(args: impl AsRef<str>, stdout: Stdio, stderr: Stdio) -> Child {
	let mut proc = std::process::Command::new("ffprobe");
	// println!("spawn ffmpeg: {}", args.as_ref());
	proc.args(args.as_ref().split_ascii_whitespace())
		.stdout(stdout)
		.stderr(stderr)
		.stdin(Stdio::null())
		.spawn()
		.unwrap()
}

pub fn get_frame_length(file: impl AsRef<Path> + Send) -> FFMpegThreadFuture<usize> {
	FFMpegThreadFuture::new(|_should_stop, progress| {
		let file = file.as_ref().to_owned();
		move || {
			let mut proc = spawn_ffprobe(
				format!(
					"-i {} -show_streams -hide_banner -select_streams a",
					file.to_str().unwrap()
				),
				Stdio::piped(),
				Stdio::null(),
			);
			// let error_message = format!("Error for {proc:?}");

			if !proc.wait().unwrap().success() {
				panic!("ffmpeg error");
			}
			let output = proc.wait_with_output().unwrap(); // ffprobe is quick

			let frames = String::from_utf8(output.stdout)
				.unwrap()
				.split('\n')
				.find(|x| x.starts_with("nb_frames="))
				.unwrap()
				.split_at(10)
				.1
				.trim()
				.parse()
				.unwrap();

			*progress.lock().unwrap() = 1.0;

			Ok(frames)
		}
	})
}

pub fn detect_silence(
	file: impl AsRef<Path> + Send,
	min_length: Duration,
	total_frames: usize,
) -> FFMpegThreadFuture<VideoPeriods> {
	FFMpegThreadFuture::new(|should_stop, progress| {
		let file = file.as_ref().to_owned();
		move || {
			let mut proc = spawn_ffmpeg(
				format!(
					"-i {} -vn -af silencedetect=noise=-50dB:d=0.05,ametadata=print:file=- -f null nul",
					file.to_str().unwrap()
				),
				Stdio::piped(),
				Stdio::null(),
			);

			let stdout = proc.stdout.as_mut().unwrap();
			let Some(output) = read_or_terminate(
				stdout,
				&should_stop,
				Some(TrackProgress { progress, total_frames }),
			) else {
				return Err(AbortReason::Killed);
			};

			if !proc.wait().unwrap().success() {
				panic!("ffmpeg error");
			}

			let mut output = output
				.split('\n')
				.map(|x| x.to_owned())
				.filter(|x| !x.starts_with("frame"))
				.collect::<Vec<String>>();
			output.insert(0, "duration=0".to_string());
			output.insert(0, "end=0".to_string());
			output.insert(0, "start=0".to_string());

			let mut filtered = 0;
			let periods = output
				.chunks_exact(3)
				.flat_map(<&[String; 3]>::try_from)
				.map(|[start, end, duration]| {
					let start =
						start.split_once('=').unwrap().1.trim().parse::<f64>().unwrap();
					let end =
						end.split_once('=').unwrap().1.trim().parse::<f64>().unwrap();
					let duration = duration
						.split_once('=')
						.unwrap()
						.1
						.trim()
						.parse::<f64>()
						.unwrap();
					(start, end, duration)
				})
				.filter(|(_start, _end, duration)| {
					// println!("duration {} actual duration {}", duration, end - start);
					if Duration::from_secs_f64(*duration) >= min_length {
						true
					} else {
						filtered += 1;
						false
					}
				})
				.map(|(start, end, _duration)| VideoPeriod::new(start, end))
				.collect::<Vec<_>>();

			// println!("filtered {} periods", filtered);

			let periods = periods
				.chunks_exact(2)
				.flat_map(|ab| {
					let [a, b] = <[VideoPeriod; 2]>::try_from(ab.clone()).unwrap();
					if b.from - a.to < 0.1 {
						[Some(VideoPeriod::new(a.from, b.to)), None]
					} else {
						[Some(a), Some(b)]
					}
				})
				.flatten()
				.collect::<Vec<_>>();

			let is_monotonous = |x: &[VideoPeriod]| {
				x.windows(2).all(|x| {
					let [a, b] = <[VideoPeriod; 2]>::try_from(x).unwrap();
					a.to <= b.from
				})
			};

			assert!(is_monotonous(&periods));

			Ok(VideoPeriods { silent_periods: periods })
		}
	})
}

pub fn split_fragments(
	file: impl AsRef<Path> + Send,
	temp_dir: impl AsRef<Path> + Send,
	video_periods: VideoPeriods,
) -> FFMpegThreadFuture<()> {
	FFMpegThreadFuture::new(|should_stop, progress| {
		let temp_dir = temp_dir.as_ref().to_owned();
		let file = file.as_ref().to_owned();
		move || {
			println!("temp dir {temp_dir:?} {}", video_periods.audible_periods().count());
			let total_periods = video_periods.audible_periods().count();
			let pad = total_periods.checked_ilog10().unwrap() as usize + 1;
			let mut filenames = Vec::new();
			let mut handles = Vec::<Child>::new();
			let mut total_processed = 0;
			for (i, period) in video_periods.audible_periods().enumerate() {
				let filename = format!("part_{:0>pad$}.mkv", i, pad = pad);
				filenames.push((filename.clone(), period.duration()));

				let cmd = format!(
					"-ss {ss} -i {input} -fflags +igndts -t {t} -c:a libopus \
					 -vf scale=-2:min(720\\,trunc(ih/2)*2),fps=30 \
					 -c:v libx264 -preset ultrafast -crf 0 -pix_fmt yuv420p -movflags faststart {output}",
					ss = period.from,
					input = file.to_str().unwrap(),
					t = period.duration(),
					output = temp_dir.join(filename).to_str().unwrap()
				);

				let command = spawn_ffmpeg(cmd, Stdio::null(), Stdio::null());

				if handles.len() < 2 {
					handles.push(command);
				} else {
					'outer: loop {
						*progress.lock().unwrap() =
							total_processed as f32 / total_periods as f32;

						for handle in handles.iter_mut() {
							if should_stop.load(Ordering::Relaxed) {
								return Err(AbortReason::Killed);
							}

							if let Some(code) = handle.try_wait().unwrap() {
								if !code.success() {
									panic!("ffmpeg error");
								}
								total_processed += 1;
								*handle = command;
								break 'outer;
							}
						}
					}
				}
			}

			println!("children {}", handles.len());

			loop {
				if should_stop.load(Ordering::Relaxed) {
					return Err(AbortReason::Killed);
				}
				if handles.iter_mut().all(|x| x.try_wait().unwrap().is_some()) {
					break;
				}
			}

			let path_to_filenames = temp_dir.join("filenames.txt");
			let path_to_filenames = path_to_filenames.to_str().unwrap();

			std::fs::write(
				path_to_filenames,
				filenames
					.into_iter()
					.map(|(x, d)| format!("file {x}\nduration {d}\n"))
					.collect::<String>(),
			)
			.unwrap();

			Ok(())
		}
	})
}

pub fn concat_fragments(
	temp_dir: impl AsRef<Path> + Send,
	output_file_name: String,
) -> anyhow::Result<FFMpegThreadFuture<()>> {
	let temp_dir = temp_dir.as_ref().to_owned();
	let path_to_filenames = temp_dir.join("filenames.txt");
	assert!(output_file_name.ends_with(".mp4"));

	if !path_to_filenames.exists() {
		return Err(anyhow::anyhow!("filenames.txt does not exist"));
	}

	Ok(FFMpegThreadFuture::new(|should_stop, _progress| {
		let output_file_path = temp_dir.join(output_file_name);
		move || {
			let mut command = spawn_ffmpeg(
				format!(
					"-fflags +igndts -threads 1 -f concat \
					-safe 0 -i {filenames} -c copy {output}",
					filenames = path_to_filenames.to_str().unwrap(),
					output = output_file_path.to_str().unwrap()
				),
				Stdio::null(),
				Stdio::piped(),
			);

			let stderr = command.stderr.as_mut().unwrap();
			let Some(_output) = read_or_terminate(
				stderr,
				&should_stop,
				// Some(TrackProgress { progress, total_frames }),
				None,
			) else {
				return Err(AbortReason::Killed);
			};

			while let Ok(None) = command.try_wait() {
				if should_stop.load(Ordering::Relaxed) {
					return Err(AbortReason::Killed);
				}
			}

			if !command.wait().unwrap().success() {
				panic!("ffmpeg error");
			}

			Ok(())
		}
	}))
}
