use std::{
	fmt::Debug,
	fs,
	marker::PhantomData,
	mem::discriminant,
	path::PathBuf,
	sync::{Arc, Mutex},
	time::Duration,
};

use crate::{
	ffmpeg::{
		self, FFMpegFuture, FFMpegReadyFuture, FFMpegThreadFuture, FFMpegThreadStatus,
		VideoPeriods,
	},
	video::{Dir, VideoFileHandleType},
};

#[derive(Debug, Clone)]
pub struct StageInfo {
	pub time_start: std::time::Instant,
	pub time_end: std::time::Instant,
	pub progress: Arc<Mutex<f32>>,
}

impl StageInfo {
	pub fn create(progress: Arc<Mutex<f32>>) -> Self {
		Self {
			time_start: std::time::Instant::now(),
			time_end: std::time::Instant::now(),
			progress,
		}
	}
}

pub enum EncodingStage {
	Idle {
		info: StageInfo,
	},
	CountFrames {
		info: StageInfo,
		frames: FFMpegThreadFuture<usize>,
	},
	DetectSilence {
		info: StageInfo,
		frames: usize,
		periods: FFMpegThreadFuture<VideoPeriods>,
	},
	Split {
		info: StageInfo,
		frames: usize,
		periods: VideoPeriods,
		ffmpeg: FFMpegThreadFuture<()>,
	},
	Concat {
		info: StageInfo,
		frames: usize,
		periods: VideoPeriods,
		ffmpeg: FFMpegThreadFuture<()>,
	},
	Move {
		info: StageInfo,
		frames: usize,
		periods: VideoPeriods,
		mv: FFMpegReadyFuture<PathBuf>,
	},
	ReEncode {
		info: StageInfo,
		frames: usize,
		periods: VideoPeriods,
		ffmpeg: Box<dyn FFMpegFuture<()> + Send + Sync>,
	},
	Cleanup {
		info: StageInfo,
		frames: usize,
		periods: VideoPeriods,
	},
	Complete {
		info: StageInfo,
		frames: usize,
		periods: VideoPeriods,
	},
}

impl Debug for EncodingStage {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			EncodingStage::Idle { info } => {
				f.debug_struct("Idle").field("info", info).finish()
			}
			EncodingStage::CountFrames { info, .. } => {
				f.debug_struct("CountFrames").field("info", info).finish()
			}
			EncodingStage::DetectSilence { info, .. } => {
				f.debug_struct("DetectSilence").field("info", info).finish()
			}
			EncodingStage::Split { info, .. } => {
				f.debug_struct("Split").field("info", info).finish()
			}
			EncodingStage::Concat { info, .. } => {
				f.debug_struct("Concat").field("info", info).finish()
			}
			EncodingStage::Move { info, .. } => {
				f.debug_struct("Move").field("info", info).finish()
			}
			EncodingStage::ReEncode { info, .. } => {
				f.debug_struct("ReEncode").field("info", info).finish()
			}
			EncodingStage::Cleanup { info, .. } => {
				f.debug_struct("Cleanup").field("info", info).finish()
			}
			EncodingStage::Complete { info, .. } => {
				f.debug_struct("Complete").field("info", info).finish()
			}
		}
	}
}

impl EncodingStage {
	pub fn advance(
		mut self,
		task: &EncodingTask<Running>,
	) -> (Option<StageInfo>, EncodingStage) {
		let self_info_clone = self.info_mut().clone();
		let disc1 = discriminant(&self);

		let now = std::time::Instant::now();
		self.info_mut().time_end = now;

		use EncodingStage::*;
		let next_stage = match self {
			Idle { .. } => {
				let frames = Self::count_frames(&task.file_handle);
				CountFrames { info: StageInfo::create(frames.get_progress()), frames }
			}
			mut stage @ CountFrames { .. } => match &mut stage {
				CountFrames { frames, .. } => match frames.poll() {
					FFMpegThreadStatus::Aborted(_) => todo!(),
					FFMpegThreadStatus::Running => stage,
					FFMpegThreadStatus::Finished(frames) => {
						let periods = Self::detect_silence(&task.file_handle, frames);
						DetectSilence {
							info: StageInfo::create(periods.get_progress()),
							frames,
							periods,
						}
					}
				},
				_ => unreachable!(),
			},
			mut stage @ DetectSilence { .. } => match &mut stage {
				DetectSilence { periods, frames, .. } => match periods.poll() {
					FFMpegThreadStatus::Aborted(_) => todo!(),
					FFMpegThreadStatus::Running => stage,
					FFMpegThreadStatus::Finished(periods) => {
						let ffmpeg =
							Self::split_fragments(&task.file_handle, periods.clone());
						Split {
							info: StageInfo::create(ffmpeg.get_progress()),
							frames: *frames,
							ffmpeg,
							periods,
						}
					}
				},
				_ => unreachable!(),
			},
			mut stage @ Split { .. } => match &mut stage {
				Split { periods, frames, ffmpeg, .. } => match ffmpeg.poll() {
					FFMpegThreadStatus::Aborted(_) => todo!(),
					FFMpegThreadStatus::Running => stage,
					FFMpegThreadStatus::Finished(_) => {
						let ffmpeg = Self::concat_fragments(&task.file_handle, *frames);
						Concat {
							info: StageInfo::create(ffmpeg.get_progress()),
							frames: *frames,
							periods: periods.clone(),
							ffmpeg,
						}
					}
				},
				_ => unreachable!(),
			},
			mut stage @ Concat { .. } => match &mut stage {
				Concat { periods, frames, ffmpeg, .. } => match ffmpeg.poll() {
					FFMpegThreadStatus::Aborted(_) => todo!(),
					FFMpegThreadStatus::Running => stage,
					FFMpegThreadStatus::Finished(_) => {
						let mv = Self::move_output(&task.file_handle);
						Move {
							info: StageInfo::create(mv.get_progress()),
							frames: *frames,
							periods: periods.clone(),
							mv,
						}
					}
				},
				_ => unreachable!(),
			},
			mut stage @ Move { .. } => match &mut stage {
				Move { periods, frames, mv, .. } => match mv.poll() {
					FFMpegThreadStatus::Aborted(_) => todo!(),
					FFMpegThreadStatus::Running => stage,
					FFMpegThreadStatus::Finished(_) => ReEncode {
						info: StageInfo::create(mv.get_progress()),
						frames: *frames,
						periods: periods.clone(),
						ffmpeg: Box::new(Self::re_encode(*frames)),
					},
				},
				_ => unreachable!(),
			},
			mut stage @ ReEncode { .. } => match &mut stage {
				ReEncode { periods, frames, ffmpeg, .. } => match ffmpeg.poll() {
					FFMpegThreadStatus::Aborted(_) => todo!(),
					FFMpegThreadStatus::Running => stage,
					FFMpegThreadStatus::Finished(_) => Cleanup {
						info: StageInfo::create(
							FFMpegReadyFuture::new(()).get_progress(),
						),
						frames: *frames,
						periods: periods.clone(),
					},
				},
				_ => unreachable!(),
			},
			Cleanup { frames, periods, .. } => {
				std::fs::read_dir(&task.file_handle.temp)
					.unwrap()
					.map(|entry| entry.unwrap())
					.filter(|entry| {
						entry
							.path()
							.file_name()
							.unwrap()
							.to_str()
							.unwrap()
							.starts_with("part")
					})
					.for_each(|entry| {
						std::fs::remove_file(entry.path()).unwrap();
					});
				Complete {
					info: StageInfo::create(FFMpegReadyFuture::new(()).get_progress()),
					frames,
					periods,
				}
			}
			stage @ Complete { .. } => stage,
		};

		(
			if disc1 == discriminant(&next_stage) { None } else { Some(self_info_clone) },
			next_stage,
		)
	}

	pub fn info_mut(&mut self) -> &mut StageInfo {
		match self {
			Self::Idle { info } => info,
			Self::CountFrames { info, .. } => info,
			Self::DetectSilence { info, .. } => info,
			Self::Split { info, .. } => info,
			Self::Concat { info, .. } => info,
			Self::Move { info, .. } => info,
			Self::ReEncode { info, .. } => info,
			Self::Cleanup { info, .. } => info,
			Self::Complete { info, .. } => info,
		}
	}

	pub fn info(&self) -> &StageInfo {
		match self {
			Self::Idle { info } => info,
			Self::CountFrames { info, .. } => info,
			Self::DetectSilence { info, .. } => info,
			Self::Split { info, .. } => info,
			Self::Concat { info, .. } => info,
			Self::Move { info, .. } => info,
			Self::ReEncode { info, .. } => info,
			Self::Cleanup { info, .. } => info,
			Self::Complete { info, .. } => info,
		}
	}

	pub fn name(&self) -> &'static str {
		match self {
			Self::Idle { .. } => "Idle",
			Self::CountFrames { .. } => "CountFrames",
			Self::DetectSilence { .. } => "DetectSilence",
			Self::Split { .. } => "Split",
			Self::Concat { .. } => "Concat",
			Self::Move { .. } => "Move",
			Self::ReEncode { .. } => "ReEncode",
			Self::Cleanup { .. } => "Cleanup",
			Self::Complete { .. } => "Complete",
		}
	}

	fn count_frames(
		file_handle: &VideoFileHandleType,
	) -> ffmpeg::FFMpegThreadFuture<usize> {
		// print!("counting frames");
		ffmpeg::get_frame_length(&file_handle.path)
	}

	fn detect_silence(
		file_handle: &VideoFileHandleType,
		total_frames: usize,
	) -> FFMpegThreadFuture<VideoPeriods> {
		// println!("detecting silence");
		ffmpeg::detect_silence(
			&file_handle.path,
			Duration::from_millis(200),
			total_frames,
		)
	}

	fn split_fragments(
		file_handle: &VideoFileHandleType,
		video_periods: VideoPeriods,
	) -> FFMpegThreadFuture<()> {
		// println!("splitting fragments");
		ffmpeg::split_fragments(&file_handle.path, &file_handle.temp, video_periods)
	}

	fn concat_fragments(
		file_handle: &VideoFileHandleType,
		_total_frames: usize,
	) -> FFMpegThreadFuture<()> {
		// println!("concatenating fragments");
		ffmpeg::concat_fragments(&file_handle.temp, "output.mp4".to_owned()).unwrap()
	}

	fn move_output(file_handle: &VideoFileHandleType) -> FFMpegReadyFuture<PathBuf> {
		// moving output is disabled
		// if enable, change 'output.mp4' to include the file name
		FFMpegReadyFuture::new(file_handle.temp.path().join("output.mp4"))

		// let output_filename = format!(
		// 	"output.mp4",
		// 	file_handle.path.file_stem().unwrap().to_str().unwrap()
		// );
		// let old_path = file_handle.temp.path().join(output_filename.clone());
		// let new_path = file_handle.temp.path().parent().unwrap().join(output_filename);
		// fs::rename(old_path, &new_path).unwrap();

		// FFMpegReadyFuture::new(new_path)
	}

	fn re_encode(_total_frames: usize) -> impl FFMpegFuture<()> {
		println!("re-encoding is not implemented");
		FFMpegReadyFuture::new(())
	}
}

#[derive(Debug)]
pub struct Stopped;
#[derive(Debug)]
pub struct Running;

#[derive(Debug)]
pub struct EncodingTask<T> {
	stage: Option<EncodingStage>,
	completed_stages: Arc<Mutex<Vec<StageInfo>>>,
	file_handle: VideoFileHandleType,
	_marker: PhantomData<T>,
}

impl EncodingTask<Stopped> {
	pub fn new(file_handle: VideoFileHandleType) -> Self {
		Self {
			stage: EncodingStage::Idle {
				info: StageInfo::create(FFMpegReadyFuture::new(()).get_progress()),
			}
			.into(),
			completed_stages: Arc::new(Mutex::new(Vec::new())),
			file_handle,
			_marker: Default::default(),
		}
	}

	pub fn start(self) -> EncodingTask<Running> {
		EncodingTask {
			stage: self.stage,
			completed_stages: self.completed_stages.clone(),
			file_handle: self.file_handle,
			_marker: Default::default(),
		}
	}
}

impl EncodingTask<Running> {
	pub fn poll(&mut self) -> &EncodingStage {
		let completed_stages = self.completed_stages.clone();
		let mut completed_stages = completed_stages.lock().unwrap();

		let stage = self.stage.take().unwrap();
		let new_stage = match stage.advance(self) {
			(_, stage @ EncodingStage::Complete { .. }) => {
				// println!("complete");
				stage
			}
			(Some(stage_info), next_stage) => {
				// println!("completed stage {:?}", stage_info);
				completed_stages.push(stage_info);
				next_stage
			}
			(None, current_stage) => current_stage,
		};
		self.stage = new_stage.into();

		self.stage.as_ref().unwrap()
	}

	pub fn completed_stages(&self) -> Vec<StageInfo> {
		let completed_stages = self.completed_stages.clone();
		let completed_stages = completed_stages.lock().unwrap();
		completed_stages.clone()
	}
}
