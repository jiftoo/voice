// #![forbid(unused_crate_dependencies)]
#![allow(clippy::option_env_unwrap)]

pub mod cell_deref;

include!("../../include/builder_comperr.rs");

pub use builder::config;

use std::{
    borrow::Cow,
    env::var,
    fmt::{Debug, Display, Formatter},
    ops::{Deref, DerefMut},
};

use tokio::net::TcpListener;

/// Start a voice axum server. Parses the PORT environment variable for the port to listen on.
pub async fn axum_serve(router: axum::Router, default_port: u16) {
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
            // dbg!(var("PORT").map(|x| x.parse().unwrap()).unwrap_or(default_port)),
            dbg!(var("PORT").map(|x| x.parse().unwrap()).expect("PORT env missing!")),
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
    VideoInput,
    // RemoteFileIdentifier identify the parent file
    VideoOutput(RemoteFileIdentifier),
    VideoAnalysis(RemoteFileIdentifier),
    Waveform(RemoteFileIdentifier),
}

impl RemoteFileKind {
    pub fn as_dir_name(&self) -> &'static str {
        match self {
            Self::VideoInput => "input",
            Self::VideoOutput(_) => "output",
            Self::VideoAnalysis(_) => "analyse",
            Self::Waveform(_) => "waveform",
        }
    }
}

pub mod debug_remote {
    use std::path::{Path, PathBuf};

    use super::*;

    pub async fn file_manager() -> impl RemoteFileManager {
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
                RemoteFileKind::VideoOutput(hash)
                | RemoteFileKind::Waveform(hash)
                | RemoteFileKind::VideoAnalysis(hash) => hash,
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

pub mod yandex_remote {
    use std::time::Duration;

    use self::cell_deref::OnceCellDeref;

    use super::*;
    use aws_config::Region;
    use aws_sdk_s3 as s3;
    use s3::{config::Credentials, presigning::PresigningConfig};

    static CONFIG: OnceCellDeref<crate::config::VoiceSharedConfig> = OnceCellDeref::const_new();

    pub async fn file_manager() -> impl RemoteFileManager {
        CONFIG
            .get_or_init(|| async {
                toml::from_str(&std::fs::read_to_string("./shared-config.toml").unwrap()).unwrap()
            })
            .await;

        let sdk_config = aws_config::from_env()
            .endpoint_url(CONFIG.endpoint_url.clone())
            .region(Region::new(CONFIG.region.clone()))
            .credentials_provider(Credentials::new(
                CONFIG.aws_id.clone(),
                CONFIG.aws_secret.clone(),
                None,
                None,
                "yandex",
            ))
            .load()
            .await;

        YandexRemoteManager {
            aws_client: s3::Client::new(&sdk_config),
            bucket_name: CONFIG.bucket_name.clone(),
        }
    }

    #[derive(Debug)]
    pub struct YandexRemoteManager {
        aws_client: s3::Client,
        bucket_name: String,
    }

    impl YandexRemoteManager {
        fn bucket_path(hash: &RemoteFileIdentifier, kind: RemoteFileKind) -> String {
            format!("{}/{}", hash, kind.as_dir_name())
        }
    }

    impl RemoteFile {
        fn bucket_path(&self) -> String {
            YandexRemoteManager::bucket_path(self.identifier(), self.kind)
        }
    }

    #[async_trait::async_trait]
    impl RemoteFileManager for YandexRemoteManager {
        async fn upload_file(
            &self,
            file: &[u8],
            kind: RemoteFileKind,
        ) -> Result<RemoteFile, RemoteFileManagerError> {
            // make a new hash or use the same hash as the parent for derived files
            let hash = match kind {
                RemoteFileKind::VideoOutput(hash)
                | RemoteFileKind::Waveform(hash)
                | RemoteFileKind::VideoAnalysis(hash) => hash,
                RemoteFileKind::VideoInput => RemoteFileIdentifier::digest(file),
            };

            println!("uploading file to {}: {}/{}", self.bucket_name, hash, kind.as_dir_name());

            if let Ok(file) = self.get_file(&hash, kind).await {
                println!("file already exists");
                return Ok(file);
            }

            let remote_file = RemoteFile::new(kind, hash);
            let path = remote_file.bucket_path();

            self.aws_client
                .put_object()
                .bucket(&self.bucket_name)
                .key(&path)
                .body(file.to_vec().into())
                .send()
                .await
                .map_err(|x| {
                    println!("failed to upload file: {}", x);
                    RemoteFileManagerError::WriteError
                })?;

            Ok(remote_file)
        }

        async fn get_file(
            &self,
            name: &RemoteFileIdentifier,
            kind: RemoteFileKind,
        ) -> Result<RemoteFile, RemoteFileManagerError> {
            let remote_file = RemoteFile::new(kind, *name);
            let path = remote_file.bucket_path();

            if (self.aws_client.head_object().bucket(&self.bucket_name).key(&path).send().await)
                .is_ok()
            {
                Ok(remote_file)
            } else {
                Err(RemoteFileManagerError::ReadError)
            }
        }

        async fn load_file(&self, file: &RemoteFile) -> Result<Vec<u8>, RemoteFileManagerError> {
            let path = file.bucket_path();

            let response = self
                .aws_client
                .get_object()
                .bucket(&self.bucket_name)
                .key(&path)
                .send()
                .await
                .map_err(|x| {
                    println!("failed to read file: {}", x);
                    RemoteFileManagerError::ReadError
                })?;

            let bytes = response
                .body
                .collect()
                .await
                .map(|x| x.to_vec())
                .map_err(|_| RemoteFileManagerError::ReadError)?;

            Ok(bytes)
        }

        async fn delete_file(&self, file: &RemoteFile) -> Result<(), RemoteFileManagerError> {
            self.aws_client
                .delete_object()
                .bucket(&self.bucket_name)
                .key(&file.bucket_path())
                .send()
                .await
                .map_err(|_| RemoteFileManagerError::ReadError)?;

            Ok(())
        }

        async fn file_url(&self, file: &RemoteFile) -> FileUrl {
            self.aws_client
                .get_object()
                .bucket(&self.bucket_name)
                .key(&file.bucket_path())
                .presigned(PresigningConfig::expires_in(Duration::from_secs(60 * 60)).unwrap())
                .await
                .map(|x| FileUrl(x.uri().parse().unwrap()))
                .expect("failed to generate presigned url")
        }

        async fn public_file_url(&self, file: &RemoteFile) -> Option<FileUrl> {
            Some(self.file_url(file).await)
        }
    }
}
