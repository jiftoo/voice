use std::io::Read;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use voice_shared::{
	FileUrl, RemoteFile, RemoteFileIdentifier, RemoteFileKind, RemoteFileManager,
	RemoteFileManagerError,
};

pub struct WaveformCreator<T: RemoteFileManager> {
	file_manager: T,
}

impl<T: RemoteFileManager> WaveformCreator<T> {
	pub fn new(file_manager: T) -> Self {
		Self { file_manager }
	}

	/// get existing or generate new waveform
	pub async fn get_waveform(
		&self,
		input_file: &RemoteFileIdentifier,
	) -> Result<Vec<u8>, RemoteFileManagerError> {
		println!("get waveform");
		if let Ok(file) = self
			.file_manager
			.get_file(input_file, RemoteFileKind::Waveform(*input_file))
			.await
		{
			println!("waveform already exists");
			return self.file_manager.load_file(&file).await;
		}

		let video_file =
			self.file_manager.get_file(input_file, RemoteFileKind::VideoInput).await?;
		println!("video file found");

		let waveform_data = self.generate_waveform(&video_file).await?;
		println!("waveform generated {}", waveform_data.len());

		let waveform_remote_file = self
			.file_manager
			.upload_file(
				&waveform_data,
				RemoteFileKind::Waveform(*video_file.identifier()),
			)
			.await?;
		println!("waveform uploaded");

		self.file_manager.load_file(&waveform_remote_file).await
	}

	/// this generates the waveform unconditionally
	async fn generate_waveform(
		&self,
		input_file: &RemoteFile,
	) -> Result<Vec<u8>, RemoteFileManagerError> {
		// ffmpeg -i mit.webm -filter_complex "aformat=channel_layouts=mono,showwavespic=s=6384x128:draw=full:colors=#ffffff" -frames:v 1 -c:v png -f image2pipe -
		// magick - -gravity Center -background white -splice 0x1 -
		// magick - -trim wave.png

		let file_url =
			self.file_manager.file_url(input_file).await.to_string_for_ffmpeg();

		println!("file url: {file_url}");

		fn make_child_error(x: impl ToString) -> RemoteFileManagerError {
			RemoteFileManagerError::ChildError(x.to_string().into())
		}

		/// helper function to pipe output of one command to another
		/// also manages exit codes
		async fn pipe_output(
			mut to: Command,
			what: &[u8],
		) -> Result<Vec<u8>, RemoteFileManagerError> {
			let mut handle = to.spawn().map_err(make_child_error)?;
			let mut stdin = handle.stdin.take().unwrap();
			stdin.write_all(what).map_err(make_child_error)?;
			// also hangs if you don't drop stdin
			drop(stdin);
			// .wait_with_output() also reads the stdout to end
			// magick hangs unless you do it (e.g. calling .wait() first)
			let output = handle.wait_with_output().unwrap();
			if output.status.success() && output.stderr.is_empty() {
				Ok(output.stdout)
			} else {
				println!("command failed: {:?}", String::from_utf8_lossy(&output.stderr));
				Err(RemoteFileManagerError::ChildError(
					"Failed to execute command".into(),
				))
			}
		}

		// i like blocks
		let waveform_png = {
			println!("executing ffmpeg");
			let ffmpeg_output = build_ffmpeg_command(file_url.as_str())
				.output()
				.map_err(make_child_error)?;
			let stderr = String::from_utf8_lossy(&ffmpeg_output.stderr);
			if !ffmpeg_output.status.success() {
				println!("ffmpeg stderr: {stderr}");
				return Err(RemoteFileManagerError::ChildError(
					"Failed to execute command".into(),
				));
			}
			let ffmpeg_output = ffmpeg_output.stdout;

			let magick_draw_line_output =
				pipe_output(build_magick_draw_line_command(), &ffmpeg_output).await?;

			pipe_output(build_magick_trim_command(), &magick_draw_line_output).await?
		};

		Ok(waveform_png)
	}
}

fn build_ffmpeg_command(file_url: &str) -> Command {
	// pretty much arbitrary
	const WAVEFORM_DIMENSIONS: &str = "6384x128";

	let mut command = Command::new("ffmpeg");
	command
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.args(["-loglevel", "error", "-hide_banner", "-y", "-i"])
		.arg(file_url)
		.arg("-filter_complex")
		.arg(format!(
			"aformat=channel_layouts=mono,showwavespic=s={}:draw=full:colors=#ffffff",
			WAVEFORM_DIMENSIONS
		))
		.args(["-frames:v", "1", "-c:v", "png", "-f", "image2pipe", "-"]);
	command
}

fn build_magick_draw_line_command() -> Command {
	let mut command = Command::new("magick");
	command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).args([
		"png:-",
		"-gravity",
		"Center",
		"-background",
		"white",
		"-splice",
		"0x1",
		"png:-",
	]);
	command
}

fn build_magick_trim_command() -> Command {
	let mut command = Command::new("magick");
	command
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.args(["png:-", "-trim", "png:-"]);
	command
}
