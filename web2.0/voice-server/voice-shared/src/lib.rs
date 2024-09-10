#![forbid(unused_crate_dependencies)]
#![allow(clippy::option_env_unwrap)]

pub mod trigger;

use std::{
	borrow::Cow,
	env::var,
	fmt::{Debug, Display, Formatter},
	ops::{Deref, DerefMut},
};

use tokio::net::TcpListener;

/// Start a voice axum server. Parses the PORT environment variable for the port to listen on.
pub async fn axum_serve(router: axum::Router) {
	println!(
		"Launched {:?}",
		std::env::current_exe()
			.map(|x| x.file_name().map(|x| x.to_os_string()))
			.ok()
			.flatten()
			.unwrap()
	);

	// enable shared layers
	let router = router
		.layer(tower_http::cors::CorsLayer::permissive())
		.layer(tower_http::compression::CompressionLayer::new().br(true));

	axum::serve(
		TcpListener::bind((
			"0.0.0.0",
			if cfg!(debug_assertions) {
				dbg!(var("PORT").map(|x| x.parse().unwrap()).unwrap_or(3000))
			} else {
				dbg!(var("PORT").map(|x| x.parse().unwrap()).expect("PORT env missing!"))
			},
		))
		.await
		.unwrap(),
		router,
	)
	.await
	.unwrap();
}

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
/// this used to be a hash and a few magic bytes. now this is just a random 256bit number.
pub struct RemoteFileIdentifier([u8; 32]);

pub const EMPTY_REMOTE_FILE_IDENTIFIER: RemoteFileIdentifier = RemoteFileIdentifier([0u8; 32]);

impl AsRef<[u8]> for RemoteFileIdentifier {
	fn as_ref(&self) -> &[u8] {
		&self.0
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

		let mut data = [0; 32];
		hex::decode_to_slice(&s, &mut data).map_err(|_| ())?;

		Ok(Self(data))
	}
}

impl TryFrom<&[u8]> for RemoteFileIdentifier {
	type Error = ();

	fn try_from(value: &[u8]) -> std::result::Result<Self, Self::Error> {
		let Ok(data) = value.try_into() else {
			return Err(());
		};

		Ok(Self(data))
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
	async fn load_file(&self, file: &RemoteFile) -> Result<Vec<u8>, RemoteFileManagerError>;
	async fn delete_file(&self, file: &RemoteFile) -> Result<(), RemoteFileManagerError>;

	/// Returs the url of the file
	///
	/// The url should be accessible by any part of the application.
	/// The url is not guaranteed to be a direct link to a local file.
	/// Callers of this function are to assume that the url always contains a file
	/// and are to handle the access to the file based on the schema of the url.
	async fn file_url(&self, file: &RemoteFile) -> FileUrl;

	/// Returs the url of the file which is accessible by the user directly
	/// useful if the backend is some sort of public hosting
	async fn public_file_url(&self, file: &RemoteFile) -> Option<FileUrl>;
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

	/// ffmpeg is smart enough to handle http or file schemas
	/// but not smart enough to follow the standard for file urls
	/// it doesn't understand the forward slashes after the protocol.
	/// ffmpeg is in fact brain damaged
	pub fn to_string_for_ffmpeg(&self) -> String {
		if self.as_url().scheme() == "file" {
			if cfg!(windows) {
				// remove the leading 'aboslute' slash on windows in case there's an aboslute url and it's windows-style absolute
				// file:///C:/Users
				//        | this one
				self.as_str().replace("file:///", "file:").replace("file://", "file:")
			} else {
				self.as_str().replace("file://", "file:")
			}
		} else {
			self.as_str().to_string()
		}
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
	VideoInput(RemoteFileIdentifier),
	VideoOutput(RemoteFileIdentifier),
	VideoAnalysis(RemoteFileIdentifier),
	Waveform(RemoteFileIdentifier),
}

impl RemoteFileKind {
	pub fn as_dir_name(&self) -> &'static str {
		match self {
			Self::VideoInput(_) => "input",
			Self::VideoOutput(_) => "output",
			Self::VideoAnalysis(_) => "analyse",
			Self::Waveform(_) => "waveform",
		}
	}
}

pub mod debug_remote {
	use std::path::{Path, PathBuf};

	use super::*;

	pub async fn file_manager(root: impl AsRef<Path>) -> impl RemoteFileManager {
		// "D:\\Coding\\rust\\voice\\web2.0\\voice-server\\debug_bucket",
		debug_remote::DebugRemoteManager::new(root)
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
			let bucket_dir_name: PathBuf = file.identifier().to_string().into();
			let bucket_path = bucket_dir_name.join(file.kind.as_dir_name());
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
				RemoteFileKind::VideoOutput(id)
				| RemoteFileKind::Waveform(id)
				| RemoteFileKind::VideoAnalysis(id)
				| RemoteFileKind::VideoInput(id) => id,
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
				tokio::fs::try_exists(self.make_file_path(&RemoteFile::new(kind, *name))).await
			{
				Ok(RemoteFile::new(kind, *name))
			} else {
				Err(RemoteFileManagerError::ReadError)
			}
		}

		async fn load_file(&self, file: &RemoteFile) -> Result<Vec<u8>, RemoteFileManagerError> {
			tokio::fs::read(self.make_file_path(file)).await.map_err(|x| {
				println!("failed to read file: {x:?} {}", self.make_file_path(file).display());
				RemoteFileManagerError::ReadError
			})
		}

		async fn delete_file(&self, file: &RemoteFile) -> Result<(), RemoteFileManagerError> {
			tokio::fs::remove_file(self.make_file_path(file))
				.await
				.map_err(|_| RemoteFileManagerError::ReadError)
		}

		async fn file_url(&self, file: &RemoteFile) -> FileUrl {
			FileUrl(url::Url::from_file_path(self.make_file_path(file)).unwrap())
		}

		async fn public_file_url(&self, _: &RemoteFile) -> Option<FileUrl> {
			None
		}
	}
}
pub mod yandex_mount_remote {
	use std::path::Path;

	use super::*;

	pub async fn file_manager(bucket_mount: impl AsRef<Path>) -> impl RemoteFileManager {
		debug_remote::DebugRemoteManager::new(bucket_mount)
	}

	#[derive(Debug)]
	pub struct YandexRemoteManager(debug_remote::DebugRemoteManager);

	#[async_trait::async_trait]
	impl RemoteFileManager for YandexRemoteManager {
		async fn upload_file(
			&self,
			file: &[u8],
			kind: RemoteFileKind,
		) -> Result<RemoteFile, RemoteFileManagerError> {
			self.0.upload_file(file, kind).await
		}

		async fn get_file(
			&self,
			name: &RemoteFileIdentifier,
			kind: RemoteFileKind,
		) -> Result<RemoteFile, RemoteFileManagerError> {
			self.0.get_file(name, kind).await
		}

		async fn load_file(&self, file: &RemoteFile) -> Result<Vec<u8>, RemoteFileManagerError> {
			self.0.load_file(file).await
		}

		async fn delete_file(&self, file: &RemoteFile) -> Result<(), RemoteFileManagerError> {
			self.0.delete_file(file).await
		}

		async fn file_url(&self, file: &RemoteFile) -> FileUrl {
			self.0.file_url(file).await
		}

		async fn public_file_url(&self, file: &RemoteFile) -> Option<FileUrl> {
			self.0.public_file_url(file).await
		}
	}
}
