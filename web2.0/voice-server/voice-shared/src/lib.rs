use std::{
	borrow::Cow,
	future::Future,
	ops::{Deref, Range},
	sync::Arc,
};

pub enum VoiceError {
	NetworkError,
	HttpError(u16),
	InvalidFileType { expected: RemoteFileKind, got: RemoteFileKind },
	InvalidFileContents(String),
	Internal(Cow<'static, str>),
}

pub type Result<T> = core::result::Result<T, VoiceError>;

pub struct Sha256Hash {
	hash: String,
}

pub trait RemoteManager: Sized {
	fn upload_file(
		&self,
		file: &[u8],
	) -> impl Future<Output = crate::Result<RemoteFile<Self>>> + Send;
	fn delete_file(
		&self,
		file: &RemoteFile<Self>,
	) -> impl Future<Output = crate::Result<()>> + Send;
	fn load_file(
		&self,
		file: &RemoteFile<Self>,
	) -> impl Future<Output = crate::Result<Vec<u8>>> + Send;

	fn file_url(&self, file: &RemoteFile<Self>) -> String;
}

pub struct RemoteFile<T: RemoteManager> {
	remote_manager: Arc<T>,
	kind: RemoteFileKind,
	path: String,
	name: Sha256Hash,
}

impl<T: RemoteManager> RemoteFile<T> {
	pub fn new(
		remote_manager: T,
		kind: RemoteFileKind,
		path: String,
		name: Sha256Hash,
	) -> Self {
		Self { remote_manager: remote_manager.into(), kind, name, path }
	}
}

pub enum RemoteFileKind {
	VideoInput,
	VideoOutput,
	Waveform,
}

impl<T: RemoteManager> RemoteFile<T> {
	pub async fn url(&self) -> String {
		self.remote_manager.file_url(self)
	}

	pub async fn load(&self) -> crate::Result<Vec<u8>> {
		self.remote_manager.load_file(self).await
	}

	pub async fn delete(self) -> crate::Result<()> {
		self.remote_manager.delete_file(&self).await
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

pub struct VoiceTaskData<T: RemoteManager> {
	audible_intervals: Option<Vec<Range<f32>>>,
	inaudible_intervals: Option<Vec<Range<f32>>>,
	video_file: Option<RemoteFile<T>>,
}

pub struct VoiceTask<T: RemoteManager> {
	input: RemoteFile<T>,
	options: VoiceTaskOptions,
	state: VoiceTaskState,
	data: VoiceTaskData<T>,
}

pub trait Voice<T: RemoteManager> {
	/// upload file or return an existing one
	fn upload_file(
		&self,
		file: &[u8],
	) -> impl Future<Output = crate::Result<RemoteFile<T>>> + Send;
	fn get_waveform(
		&self,
		file: &RemoteFile<T>,
	) -> impl Future<Output = crate::Result<RemoteFile<T>>> + Send;
	/// start a new task or return an existing one
	fn process_file(
		&self,
		file: &RemoteFile<T>,
	) -> impl Future<Output = crate::Result<VoiceTask<T>>> + Send;
}

pub mod debug_remote {
	use std::path::{Path, PathBuf};

	use super::*;

	pub struct DebugRemoteManager {
		root: PathBuf,
	}

	impl DebugRemoteManager {
		pub fn new(root: impl AsRef<Path>) -> Self {
			Self { root: root.as_ref().to_path_buf() }
		}
	}

	impl RemoteManager for Arc<DebugRemoteManager> {
		fn upload_file(
			&self,
			file: &[u8],
		) -> impl Future<Output = crate::Result<RemoteFile<Self>>> + Send {
			let hash = Sha256Hash { hash: sha256::digest(file) };
			let path = self.root.join(&hash.hash);
			let file = tokio::fs::write(path, file);
			async move {
				file.await
					.map_err(|_| VoiceError::Internal("failed to write file".into()))?;
				Ok(RemoteFile::new(
					Arc::clone(self),
					RemoteFileKind::VideoInput,
					"/inputs".to_string(),
					hash,
				))
			}
		}

		fn delete_file(
			&self,
			file: &RemoteFile<Self>,
		) -> impl Future<Output = crate::Result<()>> + Send {
			let path = self.root.join(&file.name.hash);
			let file = tokio::fs::remove_file(path);
			async move {
				file.await
					.map_err(|_| VoiceError::Internal("failed to delete file".into()))?;
				Ok(())
			}
		}

		fn load_file(
			&self,
			file: &RemoteFile<Self>,
		) -> impl Future<Output = crate::Result<Vec<u8>>> + Send {
			let path = self.root.join(&file.name.hash);
			let file = tokio::fs::read(path);
			async move {
				let file = file
					.await
					.map_err(|_| VoiceError::Internal("failed to read file".into()))?;
				Ok(file)
			}
		}

		fn file_url(&self, file: &RemoteFile<Self>) -> String {
			format!("/static/{}", self.root.join(&file.name.hash).display())
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
