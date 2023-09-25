pub mod avg;
pub mod ffmpeg;
pub mod video;

use base64::Engine;
use std::{
	ffi::OsStr,
	fmt::{Debug, Display},
	fs,
	io::{BufRead, BufReader, Cursor, Read, Write},
	path::Path,
	pin::Pin,
	process::{Child, Command, Stdio},
	sync::{
		atomic::{AtomicU32, AtomicUsize, Ordering},
		Arc,
	},
	thread,
	time::Duration,
};
use web_view::*;

use crate::{
	avg::{DisplayDuration, SlidingAverage},
	ffmpeg::get_frame_length,
	video::{AnalyzedVideoFileHandle, VideoFileHandle},
};

// fn split_streams(input: impl AsRef<Path>, frames: &[AudioFrame]) -> Vec<AudioFrame> {
// 	frames
// 		.windows(2)
// 		.filter(|ab| {
// 			// xor
// 			!ab[0].is_audible() != !ab[1].is_audible()
// 		})
// 		.enumerate()
// 		// .filter(|(i, _)| i % 2 == 0)
// 		.map(|(_, ab)| &ab[0])
// 		.cloned()
// 		.collect::<Vec<_>>()
// }

fn main() {
	let filename = "prim.mp4";

	let file = VideoFileHandle::open(filename).unwrap();

	let file = file.analyze();

	let t1 = std::time::Instant::now();

	let pad = file.silent_periods().len().checked_ilog10().unwrap() as usize + 1;
	let mut children = Vec::new();
	let mut filenames = Vec::new();
	let mut active = 0;
	let processed_n = Arc::new(AtomicUsize::new(0));
	let total_periods = file.audible_periods().count();

	ctrlc::set_handler(unsafe {
		let file_ptr = &file as *const _ as usize;
		let children_ptr = &mut children as *mut _ as usize;
		move || {
			let file_ptr = file_ptr as *mut AnalyzedVideoFileHandle;
			let children_ptr = children_ptr as *mut Vec<Child>;
			for c in (*children_ptr).iter_mut() {
				c.kill().unwrap();
				c.wait().unwrap();
			}
			println!("dropping file {:?}", (*file_ptr).temp.path());
			std::ptr::read(file_ptr).temp.close().unwrap();
		}
	})
	.unwrap();

	thread::spawn({
		const SLEEP_DURATION: Duration = Duration::from_millis(100);

		let processed_n = processed_n.clone();
		let mut average = SlidingAverage::new(200);
		let mut counter = 0;
		let mut counter_resets = 0;
		let mut last_timestamp = std::time::Instant::now();
		let mut last_processed = 0;
		let mut current_estimate = Duration::from_secs(0);
		move || loop {
			thread::sleep(SLEEP_DURATION);

			let processed = processed_n.load(Ordering::SeqCst);
			if processed > last_processed {
				last_timestamp = std::time::Instant::now();
			}
			let current_average_duration = average.push(last_timestamp.elapsed());
			last_processed = processed;

			let sub_dur = current_estimate
				.checked_sub(SLEEP_DURATION)
				.unwrap_or(Duration::from_secs(1));

			if counter == 0 || sub_dur <= Duration::from_secs(1) {
				current_estimate = current_average_duration * (total_periods - processed + 1) as u32;
			} else {
				current_estimate = sub_dur;
			}

			if counter % 5 == 0 {
				println!("processed {}/{} est. {}", processed, total_periods, DisplayDuration(current_estimate));
			}

			if processed == total_periods - 1 {
				break;
			}

			counter = match (counter, counter_resets) {
				(0, x) if x < 5 => {
					println!("reset every 3 seconds");
					counter_resets += 1;
					30
				}
				(0, _) => {
					counter_resets += 1;
					println!("reset every 10 seconds");
					100
				}
				_ => counter - 1,
			};
		}
	});

	for (i, a) in file.audible_periods().enumerate() {
		println!("{i}: {:?}", a);
	}

	for (i, period) in file.audible_periods().enumerate() {
		let filename = format!("part_{:0>pad$}.mp4", i, pad = pad);
		filenames.push(filename.clone());

		let mut command = Command::new("ffmpeg");
		let command = command
			.args(["-ss", &period.from.to_string()])
			.args([OsStr::new("-i"), file.path.as_os_str()])
			.args([
				"-fflags",
				"+igndts",
				"-t",
				&period.duration().to_string(),
				"-c:a",
				"libopus",
				// "-c:v",
				// "h264_nvenc",
				// "-preset",
				// "p1",
				"-vf",
				"scale=-2:min(720\\,trunc(ih/2)*2)",
				"-c:v",
				"libx264",
				"-preset",
				"ultrafast",
				"-crf",
				"0",
				"-pix_fmt",
				"yuv420p",
				"-r",
				"30",
				file.temp.path().join(filename).to_str().unwrap(),
			])
			.stderr(Stdio::null())
			.stdout(Stdio::null())
			.stdin(Stdio::null());
		children.push(command.spawn().unwrap());
		active += 1;
		if active > 1 {
			let mut zombies = 0;
			children.retain_mut(|x| match x.try_wait().unwrap() {
				Some(_) => {
					zombies += 1;
					processed_n.fetch_add(1, Ordering::SeqCst);
					false
				}
				None => true,
			});
			if zombies == 0 {
				children.remove(0).wait().unwrap();
				processed_n.fetch_add(1, Ordering::SeqCst);
				active -= 1;
			} else {
				active -= zombies;
			}
		}
	}

	let path_to_filenames = file.temp.path().join("filenames.txt");
	let path_to_filenames = path_to_filenames.to_str().unwrap();

	std::fs::write(path_to_filenames, filenames.into_iter().map(|x| format!("file {x}\n")).collect::<String>())
		.unwrap();

	let t2 = std::time::Instant::now();
	println!("done splitting in {}", DisplayDuration(t2 - t1));

	let mut command = Command::new("ffmpeg");
	let output_file = file
		.temp
		.path()
		.join(format!("output_{}.mp4", file.path.file_stem().unwrap().to_str().unwrap()));
	let command = command
		.args([
			"-fflags",
			"+igndts",
			"-f",
			"concat",
			"-safe",
			"0",
			"-i",
			path_to_filenames,
			// "-c:v",
			// "libx264",
			// "-preset",
			// "fast",
			"-c:v",
			"copy",
			"-r",
			"30",
			output_file.to_str().unwrap(),
		])
		.stderr(Stdio::inherit())
		.stdout(Stdio::null())
		.stdin(Stdio::null())
		.output()
		.unwrap();

	println!("done concatenating in {}", DisplayDuration(t2.elapsed()));

	let stem = file.path.file_name().unwrap().to_str().unwrap();
	let mut new_path = output_file
		.parent()
		.unwrap()
		.parent()
		.unwrap()
		.join(format!("output_{stem}"));
	std::fs::rename(&output_file, &new_path).unwrap();

	println!("done moving {:?}", new_path);

	std::mem::forget(file);
	return;

	let mut command = Command::new("ffmpeg");
	let command = command
		.args([
			"-i",
			&new_path.to_str().unwrap().to_owned(),
			"-c:v",
			"libx264",
			"-preset",
			"fast",
			"-crf",
			"30",
			"-r",
			"30",
			{
				new_path.set_file_name(format!("{}_reenc.mp4", new_path.file_name().unwrap().to_str().unwrap()));
				new_path.to_str().unwrap()
			},
		])
		.stderr(Stdio::null())
		.stdout(Stdio::null())
		.stdin(Stdio::null())
		.output()
		.unwrap();

	println!("done re-encoding {:?}", new_path);
	println!("total: {:?}", t1.elapsed());

	// println!("frames: {:?}", frames);

	// let divs = frames
	// 	.iter()
	// 	.map(|x| {
	// 		if !x.is_audible() {
	// 			(r#"<div class="frame zero"></div>"#).to_string()
	// 		} else {
	// 			format!(r#"<div class="frame" style="height: {}%"></div>"#, x.level() * 100.0)
	// 		}
	// 	})
	// 	.collect::<Vec<_>>();

	// let html = include_str!("../index.html");
	// let html = html.replace("{frames}", &divs.join("\n"));

	// let video = fs::read(filename).unwrap();
	// let video = base64::engine::general_purpose::STANDARD_NO_PAD.encode(video);
	// let html = html.replace("{video}", &video);

	// let bounds = split_streams(filename, frames.as_slice());

	// let first_audible = bounds[0].is_audible();

	// // let mut bounds = bounds.iter().map(|x| (x.time)).collect::<Vec<_>>();
	// // if first_audible {
	// // 	bounds.insert(0, 0.0);
	// // }
	// // let audible_regions =
	// // bounds.chunks_exact(2).map(|x| (x[0], x[1])).collect::<Vec<_>>();
	// // println!("{:?}", audible_regions);

	// let frame_bounds = bounds
	// 	.iter()
	// 	.map(|x| {
	// 		format!(
	// 			r"
	// .frame:nth-child({}) {{
	// 	background-color: blue !important;
	// 	height: 200% !important;
	// }}
	// ",
	// 			x.frame + 1
	// 		)
	// 	})
	// 	.collect::<Vec<String>>()
	// 	.join("\n");

	// let mut inaudible_regions = bounds.iter().map(|x| (x.time)).collect::<Vec<_>>();
	// if !first_audible {
	// 	inaudible_regions.insert(0, 0.0);
	// }
	// let mut inaudible_regions = inaudible_regions
	// 	.chunks_exact(2)
	// 	.map(|x| (x[0], x[1]))
	// 	.collect::<Vec<_>>();

	// loop {
	// 	let (data, n) = coalesce(inaudible_regions);
	// 	inaudible_regions = data;
	// 	println!("removed {n} regions");
	// 	if n == 0 {
	// 		break;
	// 	};
	// }
	// let inaudible_regions = format!(
	// 	"[{}]",
	// 	inaudible_regions
	// 		.into_iter()
	// 		.map(|x| format!("[{},{}]", x.0, x.1))
	// 		.collect::<Vec<_>>()
	// 		.join(",")
	// );
	// println!("{:?}", inaudible_regions);

	// let html = html.replace("{blue_frames}", &frame_bounds);
	// let html = html.replace("{inaudible_regions}", &inaudible_regions);

	// // web_view::builder()
	// // 	.title("My Project")
	// // 	.content(Content::Html(html))
	// // 	.size(600, 600)
	// // 	.resizable(false)
	// // 	.debug(true)
	// // 	.user_data(())
	// // 	.invoke_handler(|_webview, _arg| Ok(()))
	// // 	.run()
	// // 	.unwrap();
}

fn coalesce(mut input: Vec<(f32, f32)>) -> (Vec<(f32, f32)>, usize) {
	let mut removed = 0;
	input.retain(|x| {
		if x.1 - x.0 > 0.16 {
			true
		} else {
			removed += 1;
			false
		}
	});
	(input, removed)
}
