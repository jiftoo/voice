use std::{
	fs::{self},
	io,
	path::{Path, PathBuf},
};

use tempfile::TempDir;

#[derive(Debug)]
pub struct VideoFileHandle {
	pub path: PathBuf,
	pub temp: tempfile::TempDir,
}

fn make_temp_dir_with_prefix(file: &Path) -> io::Result<TempDir> {
	tempfile::Builder::new().prefix(file.file_stem().unwrap()).tempdir_in("./")
}

impl VideoFileHandle {
	#[allow(clippy::result_unit_err)]
	pub fn new(path: impl AsRef<Path>) -> Result<Self, ()> {
		fs::metadata(path.as_ref()).map_err(|_| ())?;
		let temp = make_temp_dir_with_prefix(path.as_ref()).map_err(|_| ())?;
		Ok(Self { path: path.as_ref().to_path_buf(), temp })
	}
}
