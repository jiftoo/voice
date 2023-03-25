use base64::Engine;
use std::{
	ffi::OsStr,
	fmt::Debug,
	fs,
	io::{BufRead, BufReader, Cursor, Write},
	path::Path,
	process::Stdio,
};
use web_view::*;

#[derive(Clone)]
struct AudioFrame {
	frame: u32,
	time: f32,
	rms_level: f32,
}

impl Debug for AudioFrame {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_fmt(format_args!("<{}>", self.frame))
	}
}

impl AudioFrame {
	fn parse(line1: &str, line2: &str) -> Self {
		let mut line1 = line1.split_whitespace();
		let frame: u32 = line1
			.next()
			.unwrap()
			.split(':')
			.nth(1)
			.unwrap()
			.parse()
			.unwrap();
		let time: f32 = line1
			.nth(1)
			.unwrap()
			.split(':')
			.nth(1)
			.unwrap()
			.parse()
			.unwrap();
		let rms_level: f32 = line2.split('=').nth(1).unwrap().parse().unwrap();
		Self {
			frame,
			time,
			rms_level,
		}
	}

	fn level(&self) -> f32 {
		let cutoff = -40.0f32;
		(self.rms_level - cutoff) / -cutoff
	}

	fn is_audible(&self) -> bool {
		self.level() >= 0.0
	}
}

fn ffmpeg_analyze(input: impl AsRef<Path>) -> Vec<AudioFrame> {
	// ffmpeg -i input.m4a -af astats=metadata=0:reset=1,ametadata=print:key=lavfi.astats.Overall.RMS_level:file=- -f null -
	let mut proc = std::process::Command::new("ffmpeg");
	let proc = proc
		.args([
			OsStr::new("-i"),
			fs::canonicalize(input).unwrap().as_os_str(),
		])
		.args([
			"-af",
			"astats=metadata=1:reset=1,ametadata=print:key=lavfi.astats.Overall.Peak_level:file=-",
			"-f",
			"null",
		])
		.arg("nul");
	let args = proc.output().unwrap();
	let reader = BufReader::new(Cursor::new(args.stdout));
	let lines = reader.lines().map(|x| x.unwrap()).collect::<Vec<_>>();

	lines
		.chunks_exact(2)
		.map(|x| AudioFrame::parse(&x[0], &x[1]))
		.collect()
}

fn split_streams(input: impl AsRef<Path>, frames: &[AudioFrame]) -> Vec<AudioFrame> {
	frames
		.windows(2)
		.filter(|ab| {
			// xor
			!ab[0].is_audible() != !ab[1].is_audible()
		})
		.enumerate()
		// .filter(|(i, _)| i % 2 == 0)
		.map(|(_, ab)| &ab[0])
		.cloned()
		.collect::<Vec<_>>()
}

fn main() {
	//TODO: refactor
	//TODO: account for multiple streams in a video, as opposed to a sound file
	let filename = "dogen.mp3";

	let frames = ffmpeg_analyze(filename);
	let divs = frames
		.iter()
		.map(|x| {
			if !x.is_audible() {
				(r#"<div class="frame zero"></div>"#).to_string()
			} else {
				format!(
					r#"<div class="frame" style="height: {}%"></div>"#,
					x.level() * 100.0
				)
			}
		})
		.collect::<Vec<_>>();

	let html = include_str!("../index.html");
	let html = html.replace("{frames}", &divs.join("\n"));

	let video = fs::read(filename).unwrap();
	let video = base64::engine::general_purpose::STANDARD_NO_PAD.encode(video);
	let html = html.replace("{video}", &video);

	let bounds = split_streams(filename, frames.as_slice());

	let split_chunks = {
		let first_audible = bounds[0].is_audible();
		let mut bounds = bounds.iter().map(|x| (x.time)).collect::<Vec<_>>();

		if first_audible {
			bounds.insert(0, 0.0);
		}

		let _ = fs::create_dir("output");
		fs::remove_dir_all("output")
			.and_then(|_| fs::create_dir("output"))
			.unwrap();

		let mut handle = std::fs::File::create("output/chunks.txt").unwrap();

		let mut forks = vec![];
		for (i, section) in bounds.chunks_exact(2).enumerate() {
			let from = section[0];
			let to = section[1];

			let current_filename = format!("chunk_{i}.m4a");
			handle
				.write_all(format!("file {current_filename}\n").as_bytes())
				.unwrap();

			let output = std::process::Command::new("ffmpeg")
				.args([
					OsStr::new("-i"),
					fs::canonicalize(filename).unwrap().as_os_str(),
				])
				// ffmpeg -i input.m4a -ss 12.31 -c copy out.m4a
				.arg("-ss")
				.arg(from.to_string())
				.arg("-t")
				.arg((to - from).to_string())
				// .arg("-c")
				// .arg("copy")
				.arg(format!("output/{current_filename}"))
				.stdout(Stdio::null())
				.stderr(Stdio::null())
				.spawn();
			println!("From {} to {} for {}", from, to, to - from);
			forks.push(output.unwrap());
		}

		for mut fork in forks {
			fork.wait().unwrap();
		}

		Some(())
	};

	let concat_chunks = {
		let output = std::process::Command::new("ffmpeg")
			.args(["-f", "concat"])
			.args(["-i", "output/chunks.txt"])
			// .args(["-c", "copy"])
			.arg("final.mp3")
			.arg("-y")
			.stderr(Stdio::null())
			.stdout(Stdio::null())
			.output();

		// fs::remove_dir_all("output").unwrap();
		Some(())
	};

	let bounds = bounds
		.into_iter()
		.map(|x| {
			format!(
				r"
.frame:nth-child({}) {{
	background-color: blue !important;
	height: 200% !important;
}}
",
				x.frame + 1
			)
		})
		.collect::<Vec<String>>()
		.join("\n");

	let html = html.replace("{blue_frames}", &bounds);

	web_view::builder()
		.title("My Project")
		.content(Content::Html(html))
		.size(600, 600)
		.resizable(false)
		.debug(true)
		.user_data(())
		.invoke_handler(|_webview, _arg| Ok(()))
		.run()
		.unwrap();
}
