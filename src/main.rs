pub mod avg;
pub mod ffmpeg;
pub mod task;
pub mod video;

use std::{
	thread,
	time::{Duration, Instant},
};

use crate::{task::EncodingStage, video::VideoFileHandle};

fn main() {
	let Some(filename) = std::env::args().nth(1) else {
		println!("no filename provided");
		return;
	};
	// let filename = "dm2.mp4";

	let Ok(file_handle) = VideoFileHandle::new(filename) else {
		println!("file not found");
		return;
	};

	let task = task::EncodingTask::new(file_handle);

	let mut task = task.start();

	let t1 = Instant::now();
	loop {
		let stage = task.poll();
		let progress = *stage.info().progress.lock().unwrap();
		println!("{}: {:.1?}%", stage.name(), progress * 100.0);
		if matches!(stage, EncodingStage::Complete { .. }) {
			break;
		}
		thread::sleep(Duration::from_millis(500));
	}

	println!("finished in {:?}", t1.elapsed());
}
