use std::{
	ffi::OsStr,
	fmt::{Debug, Display},
	fs::{self, File},
	io::{BufRead, BufReader},
	path::Path,
	process::Stdio,
	time::Duration,
};

use crate::video::VideoFileHandle;

#[derive(Debug, Clone, Copy)]
pub struct VideoPeriod {
	pub from: f64,
	pub to: f64,
}

impl Display for VideoPeriod {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "({}, {})", self.from, self.to)
	}
}

impl VideoPeriod {
	pub fn new(from: f64, to: f64) -> Self {
		Self { from, to }
	}

	pub fn duration(&self) -> f64 {
		self.to - self.from
	}
}

pub fn get_frame_length(file: impl AsRef<Path>) -> usize {
	let mut proc = std::process::Command::new("ffmpeg");
	let proc = proc
		.args([OsStr::new("-i"), file.as_ref().as_os_str()])
		.args([
			"-c:v",
			"copy",
			"-v",
			"quiet",
			"-stats",
			"-af",
			"aresample=44100,asetnsamples=4000",
			"-f",
			"null",
		])
		.arg("nul")
		.stderr(Stdio::piped())
		.stdout(Stdio::null());
	let output = proc.output().unwrap();
	let error_message = format!("Error for {output:?}");
	String::from_utf8_lossy(&output.stderr)
		// frame=   0 ...\r
		// frame=6969 ...\r\n
		.trim()
		.split('\r')
		.last()
		.expect(&error_message)
		// frame=6969 ...
		.split_ascii_whitespace()
		.next()
		.expect(&error_message)
		// frame=...
		.split_at(6)
		.1
		// 6969
		.parse()
		.expect(&error_message)
}

pub fn detect_silence(file: impl AsRef<Path>, min_length: Duration) -> Vec<VideoPeriod> {
	// ffmpeg -i input.m4a -af astats=metadata=0:reset=1,ametadata=print:key=lavfi.astats.Overall.RMS_level:file=- -f null -
	let mut proc = std::process::Command::new("ffmpeg");
	let proc = proc
		.args([OsStr::new("-i"), file.as_ref().as_os_str()])
		.args([
			"-vn",
			"-af",
			"silencedetect=noise=-50dB:d=0.05,ametadata=print:file=-",
			"-f",
			"null",
		])
		.arg("nul")
		.stderr(Stdio::null())
		.stdout(Stdio::piped());

	let output = proc
		.output()
		.unwrap()
		.stdout
		.split(|x| *x == b'\n')
		.map(|x| String::from_utf8_lossy(x).to_string())
		.filter(|x| !x.starts_with("frame"))
		.collect::<Vec<String>>();

	let mut filtered = 0;
	let periods = output
		.chunks_exact(3)
		.flat_map(<&[String; 3]>::try_from)
		.map(|[start, end, duration]| {
			let start = start.split_once('=').unwrap().1.trim().parse::<f64>().unwrap();
			let end = end.split_once('=').unwrap().1.trim().parse::<f64>().unwrap();
			let duration = duration.split_once('=').unwrap().1.trim().parse::<f64>().unwrap();
			(start, end, duration)
		})
		.filter(|(start, end, duration)| {
			if Duration::from_secs_f64(*duration) >= min_length {
				println!("{:?} {duration}", VideoPeriod::new(*start, *end));
				true
			} else {
				filtered += 1;
				false
			}
		})
		.map(|(start, end, _duration)| VideoPeriod::new(start, end))
		.collect::<Vec<_>>();

	println!("filtered {} periods", filtered);

	let periods = periods
		.chunks_exact(2)
		.flat_map(|ab| {
			let [a, b] = <[VideoPeriod; 2]>::try_from(ab.clone()).unwrap();
			if b.from - a.to < 0.1 {
				[Some(VideoPeriod::new(a.from, b.to)), None]
			} else {
				[Some(a), Some(b)]
			}
		})
		.flatten()
		.collect::<Vec<_>>();

	let is_monotonous = |x: &[VideoPeriod]| {
		x.windows(2).all(|x| {
			let [a, b] = <[VideoPeriod; 2]>::try_from(x).unwrap();
			a.to <= b.from
		})
	};

	assert!(is_monotonous(&periods));

	periods
}
