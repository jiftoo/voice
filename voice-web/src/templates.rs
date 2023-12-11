use askama::Template;

#[derive(Template)]
#[template(path = "index.html")]
pub struct Index {
	your_files: Vec<YourFile>,
	rnd: u32,
}

impl Index {
	pub fn new() -> Self {
		Self {
			rnd: rand::random(),
			your_files: vec![
				YourFile {
					name: "video1.mp4".to_owned(),
					created: 0,
					status: YourFileStatus::Uploading,
				},
				YourFile {
					name: "video2.webm".to_owned(),
					created: 0,
					status: YourFileStatus::Processing,
				},
				YourFile {
					name: "audio.mp3".to_owned(),
					created: 0,
					status: YourFileStatus::Finished,
				},
				YourFile {
					name: "audio2.wav".to_owned(),
					created: 0,
					status: YourFileStatus::Finished,
				},
				YourFile {
					name: "video1.mp4".to_owned(),
					created: 0,
					status: YourFileStatus::Uploading,
				},
				YourFile {
					name: "video2.webm".to_owned(),
					created: 0,
					status: YourFileStatus::Processing,
				},
				YourFile {
					name: "audio.mp3".to_owned(),
					created: 0,
					status: YourFileStatus::Finished,
				},
				YourFile {
					name: "audio2.wav".to_owned(),
					created: 0,
					status: YourFileStatus::Finished,
				},
				YourFile {
					name: "video1.mp4".to_owned(),
					created: 0,
					status: YourFileStatus::Uploading,
				},
				YourFile {
					name: "video2.webm".to_owned(),
					created: 0,
					status: YourFileStatus::Processing,
				},
				YourFile {
					name: "audio.mp3".to_owned(),
					created: 0,
					status: YourFileStatus::Finished,
				},
				YourFile {
					name: "audio2.wav".to_owned(),
					created: 0,
					status: YourFileStatus::Finished,
				},
				YourFile {
					name: "video1.mp4".to_owned(),
					created: 0,
					status: YourFileStatus::Uploading,
				},
			],
		}
	}
}

pub struct YourFile {
	name: String,
	created: u64,
	status: YourFileStatus,
}

#[derive(strum::Display)]
enum YourFileStatus {
	Uploading,
	Processing,
	Analyzing,
	Encoding,
	Finished,
}
