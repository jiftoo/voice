use std::{
	fs::{self},
	io,
	path::{self, Path, PathBuf},
};

use tempfile::TempDir;

use crate::config::CONFIG;

#[derive(Debug)]
pub struct VideoFileHandle<D: Dir> {
	pub path: PathBuf,
	pub temp: D,
}

pub type VideoFileHandleType = VideoFileHandle<PathBuf>;

pub trait Dir {
	fn path(&self) -> &path::Path;
}

impl Dir for TempDir {
	fn path(&self) -> &path::Path {
		self.path()
	}
}

impl Dir for PathBuf {
	fn path(&self) -> &path::Path {
		self.as_path()
	}
}

fn make_temp_dir_with_prefix(file: impl AsRef<Path>) -> io::Result<TempDir> {
	tempfile::Builder::new()
		.prefix(file.as_ref().file_stem().unwrap())
		.tempdir_in(CONFIG.read().temp_dir_root.as_path())
}

impl VideoFileHandle<PathBuf> {
	#[allow(clippy::result_unit_err)]
	/// use a file from elsewhere
	pub fn new(path: impl AsRef<Path>) -> Result<Self, ()> {
		fs::metadata(path.as_ref()).map_err(|_| ())?;
		let temp = make_temp_dir_with_prefix(path.as_ref()).map_err(|_| ())?;
		Ok(Self { path: path.as_ref().to_path_buf(), temp: temp.into_path() })
	}

	/// put the file bytes in the temp dir as new file
	pub async fn from_file(file: &[u8], name: &str) -> Result<Self, ()> {
		let temp = make_temp_dir_with_prefix(name).map_err(|_| ())?;
		let path = temp.path().join(name);
		tokio::fs::write(&path, file).await.map_err(|_| ())?;
		Ok(Self { path, temp: temp.into_path() })
	}
}
