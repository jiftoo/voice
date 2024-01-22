pub mod config {
	use std::time::Duration;

	use serde::{Deserialize, Serialize};

	#[derive(Serialize, Deserialize)]
	pub struct VoiceFileUploadConfig {
		pub silence_cutoff: (i32, i32),
		pub skip_duration: (i32, i32),
		pub max_free_file_size: usize,
		pub max_premium_file_size: usize,
		// the server is not worth our time if it's a slowpoke
		pub reqwest_connect_timeout: Duration,
	}

	#[derive(Serialize, Deserialize)]
	pub struct VoiceWaveformGenConfig {
		#[serde(with = "waveform_dimensions_serde")]
		pub waveform_dimensions: String,
	}

	// equivalent to (u32, u32)
	mod waveform_dimensions_serde {
		use serde::{Deserialize, Deserializer, Serialize, Serializer};

		pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
		where
			D: Deserializer<'de>,
		{
			<(u32, u32)>::deserialize(deserializer).map(|(x, y)| format!("{}x{}", x, y))
		}

		pub fn serialize<S>(x: &str, serializer: S) -> Result<S::Ok, S::Error>
		where
			S: Serializer,
		{
			let mut split = x.split('x');
			let x: u32 = split.next().unwrap().parse().unwrap();
			let y: u32 = split.next().unwrap().parse().unwrap();
			(x, y).serialize(serializer)
		}
	}

	#[derive(Serialize, Deserialize)]
	pub struct VoiceAnalyzerConfig {
		#[serde(with = "silencedetect_noise_serde")]
		pub silencedetect_noise: String,
		#[serde(with = "silencedetect_duration_serde")]
		pub silencedetect_duration: String,
	}

	mod silencedetect_noise_serde {
		use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

		pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
		where
			D: Deserializer<'de>,
		{
			const DB_RANGE: std::ops::Range<i32> = -100..0;
			i32::deserialize(deserializer)
				.and_then(|x| {
					DB_RANGE.contains(&x).then_some(x).ok_or_else(|| {
						D::Error::custom(format!("value not in range ({DB_RANGE:?})"))
					})
				})
				.map(|x| format!("{}dB", x))
		}

		pub fn serialize<S>(x: &str, serializer: S) -> Result<S::Ok, S::Error>
		where
			S: Serializer,
		{
			x.strip_suffix("dB")
				.ok_or_else(|| serde::ser::Error::custom("missing dB suffix"))
				.and_then(|x| x.parse::<i32>().map_err(serde::ser::Error::custom))
				.and_then(|x| x.serialize(serializer))
		}
	}

	mod silencedetect_duration_serde {
		use serde::{ser::Error, Deserialize, Deserializer, Serialize, Serializer};

		pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
		where
			D: Deserializer<'de>,
		{
			f64::deserialize(deserializer).map(|x| x.to_string())
		}

		pub fn serialize<S>(x: &str, serializer: S) -> Result<S::Ok, S::Error>
		where
			S: Serializer,
		{
			x.parse::<f64>().map_err(S::Error::custom)?.serialize(serializer)
		}
	}

	#[derive(Serialize, Deserialize)]
	pub struct VoiceSharedConfig {
		// these will be filled in by the builder
		#[serde(skip)]
		pub aws_id: String,
		#[serde(skip)]
		pub aws_secret: String,
		pub endpoint_url: String,
		#[serde(skip)]
		// this is preseting in the YcConfig
		pub bucket_name: String,
		pub region: String,
	}
}
