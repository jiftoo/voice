use std::{
	ffi::OsStr,
	fmt::Display,
	process::{Command, Stdio},
};

use serde::{
	de::{Error, Unexpected},
	Deserialize, Serialize,
};

#[derive(Serialize, Deserialize, Default)]
pub struct YcConfig {
	pub yc_folder_name: String,
	pub service_account: ServiceAccount,
	pub bucket: BucketConfig,
	pub container_registry: ContainerRegistryConfig,
	pub serverless: ServerlessConfig,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ServiceAccount {
	pub name: String,
	pub create_if_not_exists: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub struct BucketConfig {
	pub name: String,
	pub max_size: usize,
	pub create_if_not_exists: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ContainerRegistryConfig {
	pub name: String,
	pub create_if_not_exists: bool,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ServerlessConfig {
	pub voice_file_upload: ServerlessServiceSettings,
	pub voice_waveform_gen: ServerlessServiceSettings,
	pub voice_analyzer: ServerlessServiceSettings,
}

#[derive(Serialize, Deserialize, Default)]
pub struct ServerlessServiceSettings {
	pub name: String,
	pub vcpu: VCpu,
	#[serde(deserialize_with = "MinRam::deserialize")]
	pub min_ram: MinRam,
	pub concurrency: Concurrency,
	pub cores: Cores,
}

#[derive(Serialize, Deserialize, Default)]
pub enum VCpu {
	#[serde(rename = "5%")]
	_005,
	#[serde(rename = "20%")]
	#[default]
	_020,
	#[serde(rename = "50%")]
	_050,
	#[serde(rename = "100%")]
	_100,
}

impl Display for VCpu {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::_005 => write!(f, "5%"),
			Self::_020 => write!(f, "20%"),
			Self::_050 => write!(f, "50%"),
			Self::_100 => write!(f, "100%"),
		}
	}
}

impl VCpu {
	fn as_percent_int(&self) -> u32 {
		match self {
			Self::_005 => 5,
			Self::_020 => 20,
			Self::_050 => 50,
			Self::_100 => 100,
		}
	}
}

#[derive(Serialize)]
/// min ram in MB
pub struct MinRam(u16);

impl Default for MinRam {
	fn default() -> Self {
		Self(512)
	}
}

impl MinRam {
	fn new(x: u16) -> Result<Self, &'static str> {
		if !(128..=4096).contains(&x) {
			Err("MinRam must be between 128 and 4096")
		} else {
			Ok(Self(x))
		}
	}
}

impl<'de> Deserialize<'de> for MinRam {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let x = u16::deserialize(deserializer)?;
		let y = format!("{:?}", 128..=4096);
		Self::new(x)
			.map_err(|_| D::Error::invalid_value(Unexpected::Unsigned(x.into()), &y.as_str()))
	}
}

#[derive(Serialize)]
pub struct Concurrency(u8);

impl Default for Concurrency {
	fn default() -> Self {
		Self(4)
	}
}

impl Concurrency {
	fn new(x: u8) -> Result<Self, &'static str> {
		if !(1..=16).contains(&x) {
			Err("Concurrency must be between 1 and 16")
		} else {
			Ok(Self(x))
		}
	}
}

impl<'de> Deserialize<'de> for Concurrency {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let x = u8::deserialize(deserializer)?;
		let y = format!("{:?}", 1..=16);
		Self::new(x)
			.map_err(|_| D::Error::invalid_value(Unexpected::Unsigned(x.into()), &y.as_str()))
	}
}

#[derive(Serialize, Deserialize)]
pub struct Cores(u8);

impl Default for Cores {
	fn default() -> Self {
		Self(1)
	}
}

#[derive(Debug)]
pub enum YcErrorKind {
	ResourceMissing,
	ResourceAlreadyExists,
	Unspecified,
}

pub struct YcError {
	kind: YcErrorKind,
	message: String,
}

impl std::fmt::Display for YcError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:?}: {}", self.kind, self.message)
	}
}

impl<E> From<E> for YcError
where
	E: std::error::Error,
{
	fn from(e: E) -> Self {
		Self { kind: YcErrorKind::Unspecified, message: e.to_string() }
	}
}

impl YcError {
	fn unspecified(message: impl Into<String>) -> Self {
		Self { kind: YcErrorKind::Unspecified, message: message.into() }
	}
}

pub struct Yc(YcConfig);

#[derive(Deserialize)]
struct ResourceFolderListEntry {
	name: String,
	status: String,
}

pub struct AwsSecrets {
	pub id: String,
	pub secret: String,
}

impl Yc {
	pub fn init(config: YcConfig) -> Result<Self, YcError> {
		let x: Vec<ResourceFolderListEntry> =
			serde_json::from_value(invoke_yc(["resource", "folder", "list"])?).unwrap();
		x.into_iter()
			.find(|x| x.name == config.yc_folder_name)
			.ok_or_else(|| YcError {
				kind: YcErrorKind::ResourceMissing,
				message: format!("Folder '{}' not found", config.yc_folder_name),
			})
			.and_then(|x| {
				(x.status == "ACTIVE").then_some(()).ok_or_else(|| YcError {
					kind: YcErrorKind::Unspecified,
					message: format!(
						"Folder '{}' is not 'ACTIVE' but '{}'",
						config.yc_folder_name, x.status
					),
				})
			})?;

		Ok(Self(config))
	}

	pub fn initialize_service_account(&self) -> Result<AwsSecrets, YcError> {
		// create a service account which can
		// - read/write to buckets
		// - pull from container registries
		println!("Creating service account");
		self.set_up_service_account()
	}

	pub fn initialize_cloud(self) -> Result<(), YcError> {
		// create the bucket for the voice files
		println!("Creating bucket");
		self.create_bucket()?;
		// create the container registry for the docker images
		println!("Creating container registry");
		let registry_id = self.create_container_registry()?;
		// configure docker to use the yc registry
		// idk if this has side effects
		println!("Configuring docker");
		self.configure_docker()?;
		// create the serverless services
		println!("Creating serverless services");
		let service_account_id = self
			.invoke_yc_in_folder([
				"iam",
				"service-account",
				"get",
				"--name",
				&self.0.service_account.name,
			])?
			.get("id")
			.unwrap()
			.as_str()
			.unwrap()
			.to_owned();
		self.create_and_push_serverless_service(
			&registry_id,
			crate::VOICE_ANALYZER_CONTAINER_NAME,
			&self.0.serverless.voice_analyzer,
			&service_account_id,
		)?;
		self.create_and_push_serverless_service(
			&registry_id,
			crate::VOICE_FILE_UPLOAD_CONTAINER_NAME,
			&self.0.serverless.voice_file_upload,
			&service_account_id,
		)?;
		self.create_and_push_serverless_service(
			&registry_id,
			crate::VOICE_WAVEFORM_GEN_CONTAINER_NAME,
			&self.0.serverless.voice_waveform_gen,
			&service_account_id,
		)?;

		Ok(())
	}

	fn folder_id(&self) -> Result<String, YcError> {
		let x = invoke_yc(["resource", "folder", "get", "--name", &self.0.yc_folder_name])?;
		Ok(x.get("id")
			.ok_or("'id' not present")
			.map_err(YcError::unspecified)?
			.as_str()
			.unwrap()
			.to_owned())
	}

	fn set_up_service_account(&self) -> Result<AwsSecrets, YcError> {
		let mut abort = false;
		let service_account_id = loop {
			break match self.invoke_yc_in_folder([
				"iam",
				"service-account",
				"get",
				"--name",
				&self.0.service_account.name,
			]) {
				Err(x) => match x.kind {
					YcErrorKind::ResourceMissing if self.0.service_account.create_if_not_exists => {
						if abort {
							println!("Service account creation failed.");
							return Err(x);
						}
						self.invoke_yc_in_folder([
							"iam",
							"service-account",
							"create",
							"--name",
							&self.0.service_account.name,
						])?;
						// loop around
						// return self.set_up_service_account();
						// nah i'm scared of infinite recursion here
						abort = true;
						continue;
					}
					_ => {
						return Err(x);
					}
				},
				Ok(x) => x.get("id").unwrap().as_str().unwrap().to_owned(),
			};
		};

		println!("123");

		// yc resource-manager folder add-access-binding --name voice-serverless --role ai.admin --subject serviceAccount:aje561v9360dtvmk1btv --format json
		let add_role = |role| {
			invoke_yc([
				"resource-manager",
				"folder",
				"add-access-binding",
				"--name",
				&self.0.yc_folder_name,
				"--role",
				role,
				"--subject",
				&format!("serviceAccount:{}", service_account_id),
			])
		};
		add_role("container-registry.editor")?;
		add_role("storage.editor")?;
		add_role("serverless-containers.editor")?;

		let x = self.invoke_yc_in_folder([
			"iam",
			"access-key",
			"create",
			"--service-account-id",
			&service_account_id,
		])?;

		let id = x
			.get("accessKey")
			.unwrap()
			.as_object()
			.unwrap()
			.get("keyId")
			.unwrap()
			.as_str()
			.unwrap();
		let secret = x.get("secret").unwrap().as_str().unwrap();

		Ok(AwsSecrets { id: id.to_owned(), secret: secret.to_owned() })
	}

	fn create_bucket(&self) -> Result<(), YcError> {
		match self.invoke_yc_in_folder(["storage", "bucket", "get", &self.0.bucket.name]) {
			Err(x) => match x.kind {
				YcErrorKind::ResourceMissing if self.0.bucket.create_if_not_exists => {
					self.invoke_yc_in_folder([
						"storage",
						"bucket",
						"create",
						"--name",
						&self.0.bucket.name,
						"--max-size",
						&self.0.bucket.max_size.to_string(),
					])?;
				}
				_ => return Err(x),
			},
			Ok(_) => {
				// no mention of how to update access flags,
				// and the storage class will remain unchanged
				self.invoke_yc_in_folder([
					"storage",
					"bucket",
					"update",
					"--name",
					&self.0.bucket.name,
					"--max-size",
					&self.0.bucket.max_size.to_string(),
					"--remove-website-settings",
				])?;
			}
		}
		Ok(())
	}

	fn create_container_registry(&self) -> Result<String, YcError> {
		match self.invoke_yc_in_folder([
			"container",
			"registry",
			"get",
			"--name",
			&self.0.container_registry.name,
		]) {
			Err(x) => match x.kind {
				YcErrorKind::ResourceMissing if self.0.container_registry.create_if_not_exists => {
					let x = self.invoke_yc_in_folder([
						"container",
						"registry",
						"create",
						"--name",
						&self.0.container_registry.name,
					])?;
					Ok(x.get("id").unwrap().as_str().unwrap().to_owned())
				}
				_ => return Err(x),
			},
			Ok(x) => Ok(x.get("id").unwrap().as_str().unwrap().to_owned()),
		}
	}

	fn configure_docker(&self) -> Result<(), YcError> {
		self.invoke_yc_in_folder(["container", "registry", "configure-docker"])?;
		Ok(())
	}

	fn create_and_push_serverless_service(
		&self,
		container_registry_id: &str,
		existing_image_name: &str,
		settings: &ServerlessServiceSettings,
		service_account_id: &str,
	) -> Result<(), YcError> {
		let image_list = invoke("docker", ["images", "--format", "{{.Repository}}"])?;
		if !image_list.contains(existing_image_name) {
			return Err(YcError {
				kind: YcErrorKind::ResourceMissing,
				message: format!("Image '{}' not found", existing_image_name),
			});
		}

		let tag = format!("cr.yandex/{}/{}:latest", container_registry_id, existing_image_name);
		invoke("docker", ["tag", existing_image_name, &tag])?;
		invoke("docker", ["push", &tag])?;

		match self.invoke_yc_in_folder([
			"serverless",
			"container",
			"create",
			"--name",
			&settings.name,
			"--description",
			"Voice serverless service. Auto-generated by voice-server-builder.",
		]) {
			Ok(_) | Err(YcError { kind: YcErrorKind::ResourceAlreadyExists, .. }) => (),
			Err(x) => return Err(x),
		}

		// let min_ram =
		// if settings.cores.0 > 1 {

		// }

		let core_fraction = if settings.cores.0 > 1 {
			"100".to_owned()
		} else {
			settings.vcpu.as_percent_int().to_string()
		};

		println!(
			"Aligning min_ram to 128: {} -> {}",
			settings.min_ram.0,
			(settings.min_ram.0 / 128) * 128
		);

		self.invoke_yc_in_folder([
			"serverless",
			"container",
			"revision",
			"deploy",
			"--container-name",
			&settings.name,
			"--core-fraction",
			&core_fraction,
			"--memory",
			&format!("{}MB", (settings.min_ram.0 / 128) * 128),
			"--concurrency",
			&settings.concurrency.0.to_string(),
			"--cores",
			&settings.cores.0.to_string(),
			"--execution-timeout",
			"30s",
			"--service-account-id",
			service_account_id,
			"--image",
			&tag,
		])?;

		self.invoke_yc_in_folder([
			"serverless",
			"container",
			"allow-unauthenticated-invoke",
			"--name",
			&settings.name,
		])?;

		Ok(())
	}

	fn invoke_yc_in_folder<I, S>(&self, args: I) -> Result<serde_json::Value, YcError>
	where
		I: IntoIterator<Item = S>,
		S: AsRef<OsStr>,
	{
		// i spent too much time on writing this
		let args = args.into_iter().map(|x| x.as_ref().to_owned()).chain(
			["--folder-name", &self.0.yc_folder_name]
				.into_iter()
				.map(|x| AsRef::<OsStr>::as_ref(x).to_owned()),
		);
		invoke_yc(args)
	}
}

fn invoke_yc<I, S>(args: I) -> Result<serde_json::Value, YcError>
where
	I: IntoIterator<Item = S>,
	S: AsRef<OsStr>,
{
	let mut output = Command::new("yc");
	let output = output
		.args(args)
		.args(["--format", "json-rest"])
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());

	println!("> {:?}", output);
	let output = output.output().unwrap();

	let stdout = String::from_utf8_lossy(&output.stdout);
	let stderr = String::from_utf8_lossy(&output.stderr);

	if output.status.success() {
		match serde_json::from_str::<serde_json::Value>(&stdout) {
			Ok(x) => Ok(x),
			Err(_) => Ok(stdout.into()),
		}
	} else {
		Err(YcError {
			kind: match &stderr {
				x if x.contains("already exists") => YcErrorKind::ResourceAlreadyExists,
				x if x.contains("not found") || x.contains("NotFound") => {
					YcErrorKind::ResourceMissing
				}
				_ => YcErrorKind::Unspecified,
			},
			message: stderr.into(),
		})
	}
}

fn invoke<I, S>(exec: S, args: I) -> Result<String, YcError>
where
	I: IntoIterator<Item = S>,
	S: AsRef<OsStr>,
{
	let output = Command::new(exec)
		.args(args)
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.output()
		.unwrap();

	let stdout = String::from_utf8_lossy(&output.stdout);

	if output.status.success() {
		Ok(stdout.into())
	} else {
		Err(YcError::unspecified(stdout))
	}
}
