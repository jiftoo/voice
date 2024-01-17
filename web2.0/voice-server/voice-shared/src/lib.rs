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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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

impl Display for RemoteFileIdentifier {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", hex::encode(self))
	}
}

impl Debug for RemoteFileIdentifier {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		Display::fmt(self, f)
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
		let mut hash_full = [0; 32];
		hex::decode_to_slice(sha256::digest(data), &mut hash_full).unwrap();

		let mut hash = [0; 30];
		hash.copy_from_slice(&hash_full[0..30]);

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

	/// Returs the url of the file
	///
	/// The url should be accessible by any part of the application.
	/// The url is not guaranteed to be a direct link to a local file.
	/// Callers of this function are to assume that the url always contains a file
	/// and are to handle the access to the file based on the schema of the url.
	async fn file_url(&self, file: &RemoteFile) -> FileUrl;
}

#[derive(Debug)]
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

#[derive(Debug)]
pub enum RemoteFileManagerError {
	ReadError,
	WriteError,
	ChildError(Cow<'static, str>),
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

	pub fn identifier(&self) -> &RemoteFileIdentifier {
		&self.name
	}
}

#[derive(Debug, Clone, Copy)]
pub enum RemoteFileKind {
	VideoInput,
	// RemoteFileIdentifier identify the parent file
	VideoOutput(RemoteFileIdentifier),
	Waveform(RemoteFileIdentifier),
}

impl RemoteFileKind {
	pub fn as_dir_name(&self) -> &'static str {
		match self {
			Self::VideoInput => "inputs",
			Self::VideoOutput(_) => "outputs",
			Self::Waveform(_) => "waveforms",
		}
	}
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
		debug_remote::DebugRemoteManager::new(
			"D:\\Coding\\rust\\voice\\web2.0\\voice-server\\debug_bucket",
		)
	}

	#[derive(Debug)]
	pub struct DebugRemoteManager {
		root: PathBuf,
	}

	impl DebugRemoteManager {
		pub fn new(root: impl AsRef<Path>) -> Self {
			if !root.as_ref().is_absolute() {
				// this must be true since this code is shared between multiple crates
				panic!("root must be absolute");
			}
			match std::fs::create_dir(&root) {
				Ok(_) => {}
				Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
				Err(e) => panic!("failed to create debug bucket: {}", e),
			}
			Self { root: root.as_ref().to_path_buf() }
		}

		fn make_file_path(&self, file: &RemoteFile) -> PathBuf {
			let bucket_dir_name: &Path = file.kind.as_dir_name().as_ref();
			let bucket_path = bucket_dir_name.join(file.name.to_string());
			self.root.join(bucket_path)
		}
	}

	#[async_trait::async_trait]
	impl RemoteFileManager for DebugRemoteManager {
		async fn upload_file(
			&self,
			file: &[u8],
			kind: RemoteFileKind,
		) -> Result<RemoteFile, RemoteFileManagerError> {
			// make a new hash or use the same hash as the parent for derived files
			let hash = match kind {
				RemoteFileKind::VideoOutput(hash) | RemoteFileKind::Waveform(hash) => {
					hash
				}
				RemoteFileKind::VideoInput => RemoteFileIdentifier::digest(file),
			};

			println!("uploading file: {} {:?}", hash, kind);
			if let Ok(file) = self.get_file(&hash, kind).await {
				println!("file already exists");
				return Ok(file);
			}
			let path = self.make_file_path(&RemoteFile::new(kind, hash));

			println!("writing file to {}", path.display());
			let _ = tokio::fs::create_dir_all(&path.parent().unwrap()).await;
			let _ = tokio::fs::write(&path, file).await.map_err(|x| {
				println!("failed to write file: {}", x);
				RemoteFileManagerError::WriteError
			})?;

			Ok(RemoteFile::new(kind, hash))
		}

		async fn get_file(
			&self,
			name: &RemoteFileIdentifier,
			kind: RemoteFileKind,
		) -> Result<RemoteFile, RemoteFileManagerError> {
			if let Ok(true) =
				tokio::fs::try_exists(self.make_file_path(&RemoteFile::new(kind, *name)))
					.await
			{
				Ok(RemoteFile::new(kind, *name))
			} else {
				Err(RemoteFileManagerError::ReadError)
			}
		}

		async fn load_file(
			&self,
			file: &RemoteFile,
		) -> Result<Vec<u8>, RemoteFileManagerError> {
			tokio::fs::read(self.make_file_path(file)).await.map_err(|x| {
				println!(
					"failed to read file: {x:?} {}",
					self.make_file_path(file).display()
				);
				RemoteFileManagerError::ReadError
			})
		}

		async fn delete_file(
			&self,
			file: &RemoteFile,
		) -> Result<(), RemoteFileManagerError> {
			tokio::fs::remove_file(self.make_file_path(file))
				.await
				.map_err(|_| RemoteFileManagerError::ReadError)
		}

		async fn file_url(&self, file: &RemoteFile) -> FileUrl {
			FileUrl(url::Url::from_file_path(self.make_file_path(file)).unwrap())
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
