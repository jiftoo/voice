use url::Url;
use voice_shared::{
	RemoteFileIdentifier, RemoteFileKind, RemoteFileManager, RemoteFileManagerError,
};

pub struct WaveformCreator<T: RemoteFileManager> {
	file_manager: T,
}

impl<T: RemoteFileManager> WaveformCreator<T> {
	pub fn new(file_manager: T) -> Self {
		Self { file_manager }
	}

	pub async fn get_waveform(
		&self,
		input_file: &RemoteFileIdentifier,
	) -> Result<Url, RemoteFileManagerError> {
		if let Ok(file) =
			self.file_manager.get_file(input_file, RemoteFileKind::Waveform).await
		{
			return Ok(self.file_manager.file_url(&file).await.as_url().to_owned());
		}

		let _video_file =
			self.file_manager.get_file(input_file, RemoteFileKind::VideoInput).await?;

		let waveform = [0u8; 100];

		let waveform_file =
			self.file_manager.upload_file(&waveform, RemoteFileKind::Waveform).await?;

		Ok(self.file_manager.file_url(&waveform_file).await.as_url().to_owned())
	}
}
