use std::{
	collections::HashSet,
	fmt::{Display, Formatter, Result},
	io::BufRead,
};

use tokio::io::AsyncWriteExt;

type VideoPropertyValue = String;

pub enum VideoProperty {
	Supported(VideoPropertyValue),
	Unsupported(VideoPropertyValue),
}

pub struct VideoInfo {
	pub width: u32,
	pub height: u32,
	pub fps: f32,
	pub container: VideoProperty,
	pub video_codec: Option<VideoProperty>,
	pub audio_codec: Option<VideoProperty>,
}

pub enum VideoValidity {
	Valid(VideoInfo),
	BadFile,
	TooManyStreams,
	UnsupportedCodecs(Vec<VideoPropertyValue>),
	UnsupportedContainer(String),
	UnsupportedResolution(u32, u32),
}

#[derive(Debug)]
pub struct Bounds {
	supported_audio_codecs: HashSet<VideoPropertyValue>,
	supported_video_codecs: HashSet<VideoPropertyValue>,
	supported_containers: HashSet<VideoPropertyValue>,
	max_resolution: (u32, u32),
	min_resolution: (u32, u32),
	max_fps: f32,
}

impl Bounds {
	pub fn normal() -> Self {
		Self {
			supported_audio_codecs: HashSet::from_iter(["aac".into(), "opus".into()]),
			supported_video_codecs: HashSet::from_iter([
				"h264".into(),
				"vp8".into(),
				"vp9".into(),
			]),
			supported_containers: HashSet::from_iter(["mp4".into(), "webm".into()]),
			max_resolution: (1920, 1080),
			min_resolution: (256, 144),
			max_fps: 60.0,
		}
	}

	pub fn premium() -> Self {
		Self { ..Self::normal() }
	}
}

pub async fn get_video_info(
	data: &[u8],
	bounds: Bounds,
) -> anyhow::Result<VideoValidity> {
	let mut child = tokio::process::Command::new("ffprobe")
		.args([
			"-loglevel",
			"error",
			"-show_entries",
			"stream=avg_frame_rate,width,height:codec_name:stream=codec_type:format=format_name",
			"-print_format",
			"csv",
			"-",
		])
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::null())
		.spawn()
		.map_err(|e| anyhow::anyhow!("failed to spawn ffprobe: {}", e))?;

	child
		.stdin
		.take()
		.ok_or_else(|| anyhow::anyhow!("failed to get stdin"))?
		.write_all(data)
		.await?;

	let output = child
		.wait_with_output()
		.await
		.map_err(|e| anyhow::anyhow!("failed to wait for ffprobe: {}", e))?;

	let mut codecs: (Option<VideoPropertyValue>, Option<VideoPropertyValue>) =
		(None, None);
	let mut container = None;
	let mut width = None;
	let mut height = None;
	let mut frame_rate = None;

	// check if the file has (one video stream ∨ one audio stream)
	for line in output.stdout.lines().flatten() {
		match line {
			line if line.starts_with("stream") => {
				let mut x = line.split(',').skip(1);
				let codec_type = x.next().unwrap();
				let codec_name = x.next().unwrap();
				let lwidth = x.next();
				let lheight = x.next();
				let lframe_rate = x.next();

				match codec_type {
					"video" if codecs.0.is_none() => {
						codecs.0 = Some(codec_name.into());
						width = lwidth.map(str::to_string);
						height = lheight.map(str::to_string);
						let (n, d) = lframe_rate.unwrap().split_once('/').unwrap();
						frame_rate =
							Some(n.parse::<f32>().unwrap() / d.parse::<f32>().unwrap());
					}
					"audio" if codecs.1.is_none() => codecs.1 = Some(codec_name.into()),
					"video" if codecs.0.is_some() => {
						return Ok(VideoValidity::TooManyStreams)
					}
					"audio" if codecs.1.is_some() => {
						return Ok(VideoValidity::TooManyStreams)
					}
					_ => {}
				}
			}
			line if line.starts_with("format") => {
				container =
					Some(line.split(',').skip(1).next().unwrap().replace("\"", ""));
			}
			_ => unreachable!("ffprobe output is malformed"),
		}
	}

	// check if container is supported
	let container = container.unwrap();

	if !bounds.supported_containers.iter().any(|x| container.contains(x)) {
		return Ok(VideoValidity::UnsupportedContainer(container));
	}

	// check if codecs are supported
	let mut invalid_codecs = Vec::new();
	if let Some(invalid) =
		codecs.0.as_ref().and_then(|vc| bounds.supported_video_codecs.get(vc))
	{
		invalid_codecs.push(invalid.clone());
	}
	if let Some(invalid) =
		codecs.1.as_ref().and_then(|ac| bounds.supported_audio_codecs.get(ac))
	{
		invalid_codecs.push(invalid.clone());
	}
	if !invalid_codecs.is_empty() {
		return Ok(VideoValidity::UnsupportedCodecs(invalid_codecs));
	}

	// check if resolution is supported
	let Some(width) = width.and_then(|x| x.parse::<u32>().ok()) else {
		return Ok(VideoValidity::BadFile);
	};
	let Some(height) = height.and_then(|x| x.parse::<u32>().ok()) else {
		return Ok(VideoValidity::BadFile);
	};

	if width > bounds.max_resolution.0 || height > bounds.max_resolution.1 {
		return Ok(VideoValidity::UnsupportedResolution(width, height));
	}

	Ok(VideoValidity::Valid(VideoInfo {
		width,
		height,
		fps: frame_rate.unwrap(),
		container: VideoProperty::Supported(container),
		video_codec: codecs.0.map(VideoProperty::Supported),
		audio_codec: codecs.1.map(VideoProperty::Supported),
	}))
}

impl Display for VideoValidity {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			VideoValidity::Valid(_) => write!(f, "valid"),
			VideoValidity::BadFile => write!(f, "bad file"),
			VideoValidity::TooManyStreams => write!(f, "too many streams"),
			VideoValidity::UnsupportedCodecs(codecs) => {
				write!(f, "unsupported codecs ({})", codecs.join(", "))
			}
			VideoValidity::UnsupportedContainer(container) => {
				write!(f, "unsupported container ({})", container)
			}
			VideoValidity::UnsupportedResolution(width, height) => {
				write!(f, "unsupported resolution ({}, {})", width, height)
			}
		}
	}
}
