use std::{
	fmt::Display, io, ops::Range, path::PathBuf, process::Stdio, sync::Arc,
	time::Duration,
};

use tokio::{
	io::AsyncWriteExt,
	process::{Child, Command},
};

pub struct FFmpeg {
	input: String,
}

const SILENCEDETECT_NOISE: &str = "-40dB";
const SILENCEDETECT_DURATION: &str = "0.1";

enum OutputParser {
	Start,
	End(f32),
	Duration(f32, f32),
}

impl OutputParser {
	fn next(self, line: &str) -> io::Result<(Self, Option<Range<f32>>)> {
		match self {
			OutputParser::Start => Ok((
				OutputParser::End(
					Self::parse_line(line, "start")?
						.ok_or(io::Error::new(io::ErrorKind::InvalidData, line))?,
				),
				None,
			)),
			OutputParser::End(start) => Ok((
				OutputParser::Duration(
					start,
					Self::parse_line(line, "end")?
						.ok_or(io::Error::new(io::ErrorKind::InvalidData, line))?,
				),
				None,
			)),
			OutputParser::Duration(start, end) => {
				Ok((OutputParser::Start, Some(start..end)))
			}
		}
	}

	fn parse_line(line: &str, postfix: &str) -> io::Result<Option<f32>> {
		let trimmed = line.trim();
		let starts = format!("lavfi.silence_{postfix}=");
		if trimmed.starts_with(&starts) {
			let n = line
				.split_at(starts.len())
				.1
				.parse::<f32>()
				.map_err(|x| io::Error::new(io::ErrorKind::InvalidData, x))?;
			return Ok(Some(n));
		}
		Ok(None)
	}
}

pub struct VideoAnalysis {
	pub audible: Vec<Range<f32>>,
	pub inaudible: Vec<Range<f32>>,
	pub duration: Duration,
}

impl VideoAnalysis {
	fn new(silence: Vec<Range<f32>>, duration: Duration) -> Self {
		let mut audible = Vec::new();

		let head = 0.0..silence[0].start;
		let tail = silence.last().unwrap().end..duration.as_secs_f32();

		silence.iter().fold(0.0, |prev, range| {
			audible.push(prev..range.start);
			range.end
		});

		// insert period before first silence and after last silence
		audible.insert(0, head);
		audible.push(tail);

		// filter 0 length periods
		let audible = audible.into_iter().filter(|x| x.start != x.end).collect();

		Self { audible, inaudible: silence, duration }
	}
}

#[derive(Debug, Clone)]
pub enum FFmpegError {
	FFmpeg(String),
	IO(Arc<io::Error>),
}

impl From<io::Error> for FFmpegError {
	fn from(err: io::Error) -> Self {
		Self::IO(Arc::new(err))
	}
}

impl Display for FFmpegError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::FFmpeg(str) => write!(f, "{str}"),
			Self::IO(io) => write!(f, "{io}"),
		}
	}
}

impl FFmpeg {
	pub fn new(input: voice_shared::FileUrl) -> Self {
		// don't forget to convert possible file urls to the format ffmpeg understands
		Self { input: input.to_string_for_ffmpeg() }
	}

	/// returns an array of silent periods
	pub async fn analyze_silence(&self) -> Result<VideoAnalysis, FFmpegError> {
		let mut ffmpeg = self.prepare_command();
		ffmpeg
			.arg("-vn")
			.arg("-hide_banner")
			.arg("-af")
			.arg(format!(
				"silencedetect=noise={SILENCEDETECT_NOISE}:d={SILENCEDETECT_DURATION},ametadata=mode=print:file=-"
			))
			.arg("-f")
			.arg("null")
			.arg("-");

		let mut ranges = Vec::new();
		let mut parser_state = OutputParser::Start;

		let output = ffmpeg.output().await?;
		let stdout = output.stdout;
		let stderr_text = String::from_utf8(output.stderr).unwrap();

		let status = ffmpeg.status().await.unwrap();
		if !status.success() {
			tracing::debug!("silence status: {status:?}");
			return Err(FFmpegError::FFmpeg(stderr_text));
		}

		let video_duration = {
			// coded in epic hurry
			let str = "Duration: ";
			let i = stderr_text.find(str).unwrap();
			let split =
				stderr_text.split_at(i + str.len()).1.split_once(',').unwrap().0.trim();
			let split =
				split.split(':').map(|x| x.parse::<f32>().unwrap()).collect::<Vec<_>>();
			Duration::from_secs_f32(split[0] * 3600.0 + split[1] * 60.0 + split[2])
		};

		for line in String::from_utf8(stdout).unwrap().lines() {
			// lavfi.silence_*=
			if line.starts_with("lavfi") {
				let (next_state, range) = parser_state.next(line)?;
				parser_state = next_state;
				if let Some(range) = range {
					ranges.push(range);
				}
			}
		}

		if ranges.is_empty() {
			return Err(FFmpegError::FFmpeg("Video does not contain silence".into()));
		}

		// sometimes the silencedetect doesn't output silence_end
		// if close to end of video
		if let OutputParser::End(start) = parser_state {
			ranges.push(start..video_duration.as_secs_f32());
		}

		Ok(VideoAnalysis::new(ranges, video_duration))
	}

	// TODO: rewrite for new architecture
	// pub async fn spawn_remove_silence(
	// 	&self,
	// 	analysys: &VideoAnalysis,
	// ) -> io::Result<Child> {
	// 	let keep_fragments = &analysys.audible;

	// 	if keep_fragments.is_empty() {
	// 		return Err(io::Error::new(
	// 			io::ErrorKind::InvalidInput,
	// 			"no fragments to keep",
	// 		));
	// 	}

	// 	let filter = keep_fragments
	// 		.iter()
	// 		.map(|x| format!("between(t\\,{}\\,{})", x.start, x.end))
	// 		.reduce(|a, b| format!("{}+{}", a, b))
	// 		.unwrap();

	// 	let pts_shifts = analysys
	// 		.inaudible
	// 		.iter()
	// 		.map(|x| format!("gt(T,{})*({})", x.start, x.end - x.start))
	// 		.reduce(|a, b| format!("{}+{}", a, b))
	// 		.unwrap();

	// 	let pts_expr = format!("PTS-STARTPTS-({pts_shifts})/TB",);

	// 	let vf =
	// 		format!("select='{filter}',setpts='{pts_expr}',scale='trunc(oh*a/2)*2:576'");
	// 	let af = format!("aselect='{filter}',asetpts='{pts_expr}'");

	// 	let filter_complex = format!("[0:v]{vf}[video];[0:a]{af}[audio]");

	// 	let mut ffmpeg = self.prepare_command();
	// 	ffmpeg
	// 		.arg("-progress")
	// 		.arg("-")
	// 		.arg("-loglevel")
	// 		.arg("error")
	// 		.args(["-stats_period", "0.3"])
	// 		.arg("-filter_complex_script")
	// 		.arg("pipe:0")
	// 		.arg("-map")
	// 		.arg("[video]")
	// 		.arg("-map")
	// 		.arg("[audio]")
	// 		.args(["-c:v", "libx264", "-preset", "ultrafast"])
	// 		.args(["-c:a", "libopus"])
	// 		.args(["-f", "mp4"])
	// 		.arg(&self.output);

	// 	tracing::debug!("ffmpeg: {:?}", ffmpeg);

	// 	let mut child = ffmpeg.spawn()?;

	// 	let mut stdin = child.stdin.take().unwrap();
	// 	stdin.write_all(filter_complex.as_bytes()).await.unwrap();
	// 	stdin.shutdown().await.unwrap();

	// 	Ok(child)
	// }

	/// creates an ffmpeg `Command` with null pipes and input file
	/// as input, loglevel=error, so stderr only contains errors
	/// if any
	fn prepare_command(&self) -> Command {
		let mut cmd = Command::new("ffmpeg");
		cmd.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stdin(Stdio::piped())
			.stderr(Stdio::piped())
			.arg("-i")
			.arg(&self.input)
			.kill_on_drop(true);
		cmd
	}
}
