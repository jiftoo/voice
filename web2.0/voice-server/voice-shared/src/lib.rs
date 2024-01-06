pub mod process;

use std::{
	borrow::Cow,
	fmt::{Debug, Display, Formatter},
	ops::{Deref, DerefMut, Range},
};

pub enum VoiceError {
	NetworkError,
	HttpError(u16),
	InvalidFileType { expected: RemoteFileKind, got: RemoteFileKind },
	InvalidFileContents(String),
	Internal(Cow<'static, str>),
}

// pub type Result<T> = core::result::Result<T, VoiceError>;
pub struct PrivateDebug<T>(pub T);

impl<T> Deref for PrivateDebug<T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<T> DerefMut for PrivateDebug<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

impl<T> Debug for PrivateDebug<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "<private>")
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(packed, C)]
pub struct RemoteFileIdentifier {
	hash: [u8; 30],
	magic: u8,
	check: u8,
}

impl AsRef<[u8]> for RemoteFileIdentifier {
	fn as_ref(&self) -> &[u8] {
		unsafe {
			union Convert<'a> {
				hash: &'a RemoteFileIdentifier,
				bytes: &'a [u8; 32],
			}
			Convert { hash: self }.bytes
		}
	}
}

impl std::fmt::Display for RemoteFileIdentifier {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", hex::encode(self))
	}
}

impl std::str::FromStr for RemoteFileIdentifier {
	type Err = ();

	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		if s.len() != 64 {
			return Err(());
		}

		let hash = {
			let mut hash = [0; 30];
			hex::decode_to_slice(&s[0..60], &mut hash).map_err(|_| ())?;
			hash
		};
		let magic = u8::from_str_radix(&s[60..62], 16).map_err(|_| ())?;
		if magic != 0x69 {
			return Err(());
		}
		let check = u8::from_str_radix(&s[62..64], 16).map_err(|_| ())?;
		if check != Self::check(&hash) {
			return Err(());
		}
		Ok(Self { hash, magic: 0x69, check })
	}
}

impl TryFrom<&[u8]> for RemoteFileIdentifier {
	type Error = ();

	fn try_from(value: &[u8]) -> std::result::Result<Self, Self::Error> {
		if value.len() != 32 {
			return Err(());
		}
		if value[30] != 0x69 {
			return Err(());
		}

		let hash: [u8; 30] = unsafe { value[0..30].try_into().unwrap_unchecked() };

		if value[31] != Self::check(&hash) {
			return Err(());
		}

		Ok(Self { hash, magic: 0x69, check: value[31] })
	}
}

impl RemoteFileIdentifier {
	pub fn digest(data: impl sha256::Sha256Digest) -> Self {
		let mut hash = [0; 30];
		hash.copy_from_slice(&sha256::digest(data).as_bytes()[0..30]);

		Self { hash, magic: 0x69, check: Self::check(&hash) }
	}

	// popcnt
	fn check(input: &[u8; 30]) -> u8 {
		unsafe {
			std::mem::transmute::<_, &[u64; 4]>(input)
				.iter()
				.copied()
				.map(u64::count_ones)
				.sum::<u32>() as u8
		}
	}
}

#[async_trait::async_trait]
pub trait RemoteFileManager: Sync + Send {
	async fn upload_file(
		&self,
		file: &[u8],
		kind: RemoteFileKind,
	) -> Result<RemoteFile, RemoteFileManagerError>;
	async fn get_file(
		&self,
		name: &RemoteFileIdentifier,
		kind: RemoteFileKind,
	) -> Result<RemoteFile, RemoteFileManagerError>;
	async fn load_file(
		&self,
		file: &RemoteFile,
	) -> Result<Vec<u8>, RemoteFileManagerError>;
	async fn delete_file(&self, file: &RemoteFile) -> Result<(), RemoteFileManagerError>;

	async fn file_url(&self, file: &RemoteFile) -> FileUrl;
}

pub struct FileUrl(url::Url);

impl Display for FileUrl {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl FileUrl {
	pub fn as_str(&self) -> &str {
		self.0.as_str()
	}

	pub fn as_url(&self) -> &url::Url {
		&self.0
	}
}

pub enum RemoteFileManagerError {
	ReadError,
	WriteError,
	Unspecified(Cow<'static, str>),
}

#[derive(Debug)]
pub struct RemoteFile {
	kind: RemoteFileKind,
	name: RemoteFileIdentifier,
}

impl RemoteFile {
	pub fn new(kind: RemoteFileKind, name: RemoteFileIdentifier) -> Self {
		Self { kind, name }
	}
}

#[derive(Debug)]
pub enum RemoteFileKind {
	VideoInput,
	VideoOutput,
	Waveform,
}

// public api

pub struct VoiceTaskOptions {
	denoise: bool,
	render_to_file: bool,
	silence_cutoff: i16,
	min_skip_duration: u16,
}

pub enum VoiceTaskState {
	Waiting,
	Processing,
	AnalyzedIntervals,
	EncodingVideo,

	Finished,
	Error(TaskError),
}

pub struct TaskError(String);

pub struct VoiceTaskData {
	audible_intervals: Option<Vec<Range<f32>>>,
	inaudible_intervals: Option<Vec<Range<f32>>>,
	video_file: Option<RemoteFile>,
}

pub struct VoiceTask {
	input: RemoteFile,
	options: VoiceTaskOptions,
	state: VoiceTaskState,
	data: VoiceTaskData,
}

pub mod debug_remote {
	use std::path::{Path, PathBuf};

	use super::*;

	pub fn file_manager() -> impl RemoteFileManager {
		debug_remote::DebugRemoteManager::new("./debug_bucket")
	}

	#[derive(Debug)]
	pub struct DebugRemoteManager {
		root: PathBuf,
	}

	impl DebugRemoteManager {
		pub fn new(root: impl AsRef<Path>) -> Self {
			Self { root: root.as_ref().to_path_buf() }
		}
	}

	#[async_trait::async_trait]
	impl RemoteFileManager for DebugRemoteManager {
		async fn upload_file(
			&self,
			file: &[u8],
			kind: RemoteFileKind,
		) -> Result<RemoteFile, RemoteFileManagerError> {
			let hash = RemoteFileIdentifier::digest(file);
			if let Ok(file) = self.get_file(&hash, kind).await {
				return Ok(file);
			}
			let path = self.root.join(hash.to_string()).display().to_string();
			let _ = tokio::fs::write(&path, file)
				.await
				.map_err(|_| RemoteFileManagerError::WriteError)?;
			Ok(RemoteFile::new(RemoteFileKind::VideoInput, hash))
		}

		async fn get_file(
			&self,
			name: &RemoteFileIdentifier,
			kind: RemoteFileKind,
		) -> Result<RemoteFile, RemoteFileManagerError> {
			let path = self
				.root
				.join(match kind {
					RemoteFileKind::VideoInput => "inputs",
					RemoteFileKind::VideoOutput => "outputs",
					RemoteFileKind::Waveform => "waveforms",
				})
				.join(name.to_string());
			if let Ok(true) = tokio::fs::try_exists(path).await {
				Ok(RemoteFile::new(kind, *name))
			} else {
				Err(RemoteFileManagerError::ReadError)
			}
		}

		async fn load_file(
			&self,
			file: &RemoteFile,
		) -> Result<Vec<u8>, RemoteFileManagerError> {
			let path = self.root.join(file.name.to_string());
			tokio::fs::read(path).await.map_err(|_| RemoteFileManagerError::ReadError)
		}

		async fn delete_file(
			&self,
			file: &RemoteFile,
		) -> Result<(), RemoteFileManagerError> {
			let path = self.root.join(file.name.to_string());
			tokio::fs::remove_file(path)
				.await
				.map_err(|_| RemoteFileManagerError::ReadError)
		}

		async fn file_url(&self, file: &RemoteFile) -> FileUrl {
			FileUrl(
				format!(
					"http://localhost:3002/{}",
					self.root.join(file.name.to_string()).display()
				)
				.parse()
				.unwrap(),
			)
		}
	}

	// impl Voice<Arc<DebugRemoteManager>> for Arc<DebugRemoteManager> {
	// 	fn upload_file(
	// 		&self,
	// 		file: &[u8],
	// 	) -> impl Future<Output = crate::Result<RemoteFile<Self>>> + Send {
	// 		RemoteManager::upload_file(self, file)
	// 	}

	// 	fn get_waveform(
	// 		&self,
	// 		file: &RemoteFile<Self>,
	// 	) -> impl Future<Output = crate::Result<RemoteFile<Self>>> + Send {
	// 		if !matches!(file.kind, RemoteFileKind::VideoInput) {
	// 			return async move {
	// 				Err(VoiceError::InvalidFileType {
	// 					expected: RemoteFileKind::VideoInput,
	// 					got: file.kind,
	// 				})
	// 			};
	// 		}
	// 		RemoteManager::load_file(&self, file)
	// 		async move {
	// 			file.await
	// 				.map_err(|_| VoiceError::Internal("failed to write file".into()))?;
	// 			Ok(RemoteFile::new(Arc::clone(self), RemoteFileKind::Waveform, hash))
	// 		}
	// 	}

	// 	fn process_file(
	// 		&self,
	// 		file: &RemoteFile<Self>,
	// 	) -> impl Future<Output = crate::Result<VoiceTask<Self>>> + Send {
	// 		let hash = Sha256Hash { hash: sha256::digest(file) };
	// 		let path = self.root.join(&hash.hash);
	// 		let file = tokio::fs::write(path, file);
	// 		async move {
	// 			file.await
	// 				.map_err(|_| VoiceError::Internal("failed to write file".into()))?;
	// 			Ok(VoiceTask::new(Arc::clone(self), RemoteFileKind::VideoOutput, hash))
	// 		}
	// 	}
	// }
}
