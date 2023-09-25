use std::{
	fs::{self, File},
	io,
	path::{Path, PathBuf},
	time::Duration,
};

use tempfile::TempDir;

use crate::ffmpeg::{self, VideoPeriod};

#[derive(Debug)]
pub struct VideoFileHandle {
	pub path: PathBuf,
	pub temp: tempfile::TempDir,
}

#[derive(Debug)]
pub struct AnalyzedVideoFileHandle {
	pub path: PathBuf,
	pub temp: tempfile::TempDir,
	silent_periods: Vec<VideoPeriod>,
}

fn make_temp_dir_with_prefix(file: &Path) -> io::Result<TempDir> {
	tempfile::Builder::new()
		.prefix(file.file_stem().unwrap())
		.tempdir_in("./")
}

impl VideoFileHandle {
	pub fn open(path: impl AsRef<Path>) -> Result<Self, ()> {
		fs::metadata(path.as_ref()).map_err(|_| ())?;
		let temp = make_temp_dir_with_prefix(path.as_ref()).map_err(|_| ())?;
		Ok(Self {
			path: path.as_ref().to_path_buf(),
			temp,
		})
	}

	pub fn analyze(self) -> AnalyzedVideoFileHandle {
		print!("counting frames: ");
		let frames = ffmpeg::get_frame_length(&self.path);
		println!("{}", frames);
		println!("detecting silence");
		let silent_periods = ffmpeg::detect_silence(&self.path, Duration::from_millis(200));
		AnalyzedVideoFileHandle {
			path: self.path,
			temp: self.temp,
			silent_periods,
		}
	}
}

impl AnalyzedVideoFileHandle {
	pub fn silent_periods(&self) -> &[VideoPeriod] {
		&self.silent_periods
	}

	pub fn audible_periods(&self) -> impl Iterator<Item = VideoPeriod> + '_ {
		self.silent_periods.windows(2).map(|ab| VideoPeriod {
			from: ab[0].to,
			to: ab[1].from,
		})
	}
}
