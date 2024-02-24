//! This cursed builder idea was inspired by docker's lack of support for COPYing
//! files from outside the build context, and yandex's lack of support for
//! docker-compose.

mod yc;

use std::{io::Write, path::Path, process::Stdio};

use builder::config::*;
use serde::{Deserialize, Serialize};

const BUILD_CONFIG_FILE: &str = "build-config.toml";

pub const VOICE_ANALYZER_CONTAINER_NAME: &str = "voice-analyzer";
pub const VOICE_ANALYZER_DIR: &str = "./voice-analyzer";

pub const VOICE_FILE_UPLOAD_CONTAINER_NAME: &str = "voice-file-upload";
pub const VOICE_FILE_UPLOAD_DIR: &str = "./voice-file-upload";

pub const VOICE_WAVEFORM_GEN_CONTAINER_NAME: &str = "voice-waveform-gen";
pub const VOICE_WAVEFORM_GEN_DIR: &str = "./voice-waveform-gen";

#[derive(Serialize, Deserialize, Default)]
struct BuildConfig {
	yc: yc::YcConfig,
	config: Configs,
}

#[derive(Serialize, Deserialize)]
struct Configs {
	voice_file_upload: VoiceFileUploadConfig,
	voice_waveform_gen: VoiceWaveformGenConfig,
	voice_analyzer: VoiceAnalyzerConfig,
	voice_shared: VoiceSharedConfig,
}

impl Default for Configs {
	fn default() -> Self {
		Self {
			voice_file_upload: VoiceFileUploadConfig {
				silence_cutoff: (-90, -10),
				skip_duration: (100, 250),
				max_free_file_size: 100 * 1024 * 1024,
				max_premium_file_size: 350 * 1024 * 1024,
				reqwest_connect_timeout: Default::default(),
			},
			voice_waveform_gen: VoiceWaveformGenConfig {
				waveform_dimensions: "6384x128".into(),
			},
			voice_analyzer: VoiceAnalyzerConfig {
				silencedetect_noise: "-40dB".into(),
				silencedetect_duration: "0.1".into(),
			},
			voice_shared: VoiceSharedConfig {
				aws_id: "<aws id>".into(),
				aws_secret: "<aws secret>".into(),
				endpoint_url: "https://storage.yandexcloud.net".into(),
				bucket_name: "".into(),
				region: "ru-central1".into(),
			},
		}
	}
}

const DOCKERFILE_TEMPLATE: &str = r#"
FROM rust:1.75 as prepare
	RUN apt update && \
		apt install -y --no-install-recommends mold ca-certificates %deps% && \
		rm -rf /var/lib/apt/lists/*

	WORKDIR /build

FROM prepare as builder
	COPY . .
	ENV RUSTFLAGS="-C target-feature=+crt-static --cfg=____builder"
	RUN cargo build --release --bin %project_name% --target x86_64-unknown-linux-gnu

FROM scratch
	COPY --from=prepare /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
	COPY --from=builder /build/target/x86_64-unknown-linux-gnu/release/%project_name% /application
	COPY --from=builder /build/__config.toml /config.toml
	COPY --from=builder /build/__shared-config.toml /shared-config.toml
	%copy%

ENTRYPOINT ["/application"]
"#;

const DOCKERIGNORE_TEMPLATE: &str = r#"
**/target
builder/src/main.rs
"#;

struct DockerfileTemplate(String);
struct DockerfileTemplateBuilder {
	deps: &'static [&'static str],
	project_name: &'static str,
	copy: &'static [(&'static str, &'static str)],
	config: String,
}

impl DockerfileTemplateBuilder {
	fn build(self) -> DockerfileTemplate {
		let mut dockerfile = DOCKERFILE_TEMPLATE.to_string();
		dockerfile = dockerfile.replace("%deps%", &self.deps.join(" "));
		dockerfile = dockerfile.replace("%project_name%", self.project_name);
		dockerfile = dockerfile.replace(
			"%copy%",
			&self
				.copy
				.iter()
				.map(|(from, to)| format!("COPY --from=builder {} {}", from, to))
				.collect::<Vec<_>>()
				.join("\n"),
		);
		DockerfileTemplate(dockerfile)
	}
}

#[derive(Deserialize)]
struct CargoWorkspaceToml {
	workspace: CargoWorkspaceToml2,
}

#[derive(Deserialize)]
struct CargoWorkspaceToml2 {
	members: Vec<String>,
}

struct Cleanup;

const TEMP_CONFIG: &str = "__config.toml";
const TEMP_SHARED_CONFIG: &str = "__shared-config.toml";

impl Drop for Cleanup {
	fn drop(&mut self) {
		let _ = std::fs::remove_file(TEMP_CONFIG);
		let _ = std::fs::remove_file(TEMP_SHARED_CONFIG);
	}
}

fn main() {
	// RAII cleanup
	let _ = Cleanup;

	println!("I'm the builder!");
	
	println!("Looking for '{}'...", BUILD_CONFIG_FILE);
	let Ok(build_config) = std::fs::read_to_string(BUILD_CONFIG_FILE) else {
		println!("Couldn't find '{}'.", BUILD_CONFIG_FILE);
		println!("Creating a new one:");
		let toml = toml::to_string(&BuildConfig::default()).unwrap();
		println!("{}", toml);
		println!();
		println!("Please edit the config and try again.");
		std::fs::write(BUILD_CONFIG_FILE, toml).unwrap();
		std::process::exit(1);
	};

	println!("\nConfig:\n");
	println!("{}", build_config);

	let Ok(mut build_config) = toml::from_str::<BuildConfig>(&build_config) else {
		println!("Config is invalid.");
		println!("Please make sure that the config is valid and try again.");
		std::process::exit(1);
	};

	// workaround
	build_config.config.voice_shared.bucket_name = build_config.yc.bucket.name.clone();

	println!("Looking for voice cargo workspace...");

	if !check_valid_workspace() {
		println!("Voice cargo workspace is not valid.");
		println!("Please make sure that all the voice crates are present and try again.");
		std::process::exit(1);
	}

	print!("Checking docker...");
	if !check_docker() {
		println!("Docker is not running.");
		println!("Please start docker and try again.");
		std::process::exit(1);
	}
	println!("ok");

	print!("Checking yandex cli...");
	if !check_yandex_cli() {
		println!("'yc' is not installed.");
		println!("Please install yandex cli and try again.");
		std::process::exit(1);
	}
	println!("ok");

	// preparing cloud

	let yc = match yc::Yc::init(build_config.yc) {
		Ok(x) => x,
		Err(e) => {
			println!("Failed prepare yandex cloud.");
			println!("{}", e);
			std::process::exit(1);
		}
	};

	let aws_secrets = match yc.initialize_service_account() {
		Ok(x) => x,
		Err(e) => {
			println!("Failed to initialize yandex service account.");
			println!("{}", e);
			std::process::exit(1);
		}
	};

	build_config.config.voice_shared.aws_id = aws_secrets.id;
	build_config.config.voice_shared.aws_secret = aws_secrets.secret;

	// building

	println!("Creating a temporary .dockerignore.");
	std::fs::write(".dockerignore", DOCKERIGNORE_TEMPLATE).unwrap();

	println!("Starting build.");

	std::fs::write(
		TEMP_SHARED_CONFIG,
		toml::to_string(&build_config.config.voice_shared).unwrap(),
	)
	.unwrap();

	build_docker_image(
		VOICE_ANALYZER_CONTAINER_NAME,
		DockerfileTemplateBuilder {
			config: toml::to_string(&build_config.config.voice_analyzer).unwrap(),
			deps: &["ffmpeg"],
			copy: &[("/usr/bin/ffmpeg", "/usr/bin/ffmpeg")],
			project_name: Path::new(VOICE_ANALYZER_DIR)
				.file_name()
				.unwrap()
				.to_str()
				.unwrap(),
		},
	);

	build_docker_image(
		VOICE_FILE_UPLOAD_CONTAINER_NAME,
		DockerfileTemplateBuilder {
			config: toml::to_string(&build_config.config.voice_file_upload).unwrap(),
			deps: &[],
			copy: &[],
			project_name: Path::new(VOICE_FILE_UPLOAD_DIR)
				.file_name()
				.unwrap()
				.to_str()
				.unwrap(),
		},
	);

	build_docker_image(
		VOICE_WAVEFORM_GEN_CONTAINER_NAME,
		DockerfileTemplateBuilder {
			config: toml::to_string(&build_config.config.voice_waveform_gen).unwrap(),
			deps: &["ffmpeg", "imagemagick"],
			copy: &[
				("/usr/bin/ffmpeg", "/usr/bin/ffmpeg"),
				("/usr/bin/convert", "/usr/bin/convert"),
			],
			project_name: Path::new(VOICE_WAVEFORM_GEN_DIR)
				.file_name()
				.unwrap()
				.to_str()
				.unwrap(),
		},
	);

	println!("Removing temporary .dockerignore.");
	std::fs::remove_file(".dockerignore").unwrap();

	println!("Pushing images.");

	match yc.initialize_cloud() {
		Ok(_) => {}
		Err(e) => {
			println!("Failed to initialize yandex cloud.");
			println!("{}", e);
			std::process::exit(1);
		}
	}

	println!("////////////////////////////");
	println!("Finished!");
}

fn build_docker_image(name: &str, dockerfile: DockerfileTemplateBuilder) {
	println!("Building '{name}'...");
	std::fs::write(TEMP_CONFIG, &dockerfile.config).unwrap();
	let mut proc = std::process::Command::new("docker")
		.arg("build")
		.arg("-t")
		.arg(name)
		.arg("-f")
		.arg("-")
		.arg(".")
		// .arg(dockerfile.project_name)
		.stdin(Stdio::piped())
		.stdout(Stdio::inherit())
		.stderr(Stdio::inherit())
		.spawn()
		.unwrap();
	proc.stdin.take().unwrap().write_all(dockerfile.build().0.as_bytes()).unwrap();
	let output = proc.wait_with_output().unwrap();
	if !output.status.success() {
		println!("Docker build failed.");
		println!("{}", String::from_utf8_lossy(&output.stderr));
		std::process::exit(2);
	}
}

fn check_valid_workspace() -> bool {
	let workspace_toml: CargoWorkspaceToml = match std::fs::read_to_string("./Cargo.toml")
		.map_err(|x| x.to_string())
		.and_then(|x| toml::from_str(&x).map_err(|e| e.to_string()))
	{
		Ok(x) => x,
		Err(e) => {
			println!("{}", e);
			println!("Couldn't parse voice cargo workspace.");
			println!("Please run me from the root of the voice cargo workspace.");
			std::process::exit(1);
		}
	};

	const NECESSARY_MEMBERS: &[&str] = &[
		"voice-file-upload",
		"voice-waveform-gen",
		"voice-shared",
		"voice-analyzer",
		"builder",
	];

	let mut a = NECESSARY_MEMBERS.to_vec();
	a.sort();
	let mut b = workspace_toml.workspace.members;
	b.sort();
	a == b
}

fn check_docker() -> bool {
	std::process::Command::new("docker")
		.arg("ps")
		.output()
		.is_ok_and(|x| String::from_utf8_lossy(&x.stdout).contains("CONTAINER ID"))
}

fn check_yandex_cli() -> bool {
	let output = std::process::Command::new("yc").arg("config").arg("list").output();
	let is_ok = output.as_ref().is_ok_and(|x| x.status.success());
	let check_result = output.is_ok_and(|x| {
		String::from_utf8_lossy(&x.stdout).to_lowercase().contains("cloud-id:")
	});
	if !check_result && is_ok {
		println!("yc check failed but the command exited successfully.");
	}
	check_result
}
