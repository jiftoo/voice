import {JSX, Setter, Signal, createEffect, createResource, createSignal} from "solid-js";
import {useNavigate} from "@solidjs/router";
import "./Upload.css";
import {GLOBAL_STATE} from "../globalState";
import {createResourceDebounced} from "../util";
import Button from "../components/Button";
import FileSelector from "../components/FileSelector";
import InputField from "../components/InputField";
import ProgressBar from "../components/ProgressBar";
import Switch from "../components/Switch";
import Slider from "../components/Slider";
import InfoTooltip from "../components/InfoTooltip";

type IsUrlAcceptableFileOutput = "ok" | "not-video" | "too-big" | "server-error" | "request-error";

async function isUrlAcceptableFile(url: string): Promise<IsUrlAcceptableFileOutput> {
	return fetch("http://localhost:3001/check-upload-url?premium=" + GLOBAL_STATE.premium[0](), {
		method: "POST",
		headers: {
			"Content-Type": "text/plain"
		},
		body: url
	})
		.then(res => {
			switch (res.status) {
				case 200:
					return "ok";
				case 415:
					return "not-video";
				case 413:
					return "too-big";
				default:
					console.error("Unexpected status code: " + res.status);
					return "server-error";
			}
		})
		.catch(err => {
			console.error(err);
			return "request-error";
		});
}

function mapIsUrlAcceptableFileOutputToMessage(output: IsUrlAcceptableFileOutput): string {
	switch (output) {
		case "ok":
			return "";
		case "not-video":
			return "Url is not a valid video!";
		case "too-big":
			return "Video is too big!";
		case "server-error":
			return "Server error!";
		case "request-error":
			return "Request error or timeout!";
		default:
			return "unreachable";
	}
}

type Constants = {
	silenceCutoff: {min: number; max: number};
	skipDuration: {min: number; max: number};
	maxFileSize: number;
};

function fetchConstants(isPremium: boolean, abortSignal?: AbortSignal): Promise<Constants> {
	return fetch("http://localhost:3001/constants?premium=" + isPremium, {
		signal: abortSignal
		// credentials: "include"
	}).then(res => res.json());
}

export default function Upload() {
	const navigate = useNavigate();

	const [selectedFile, setSelectedFile] = createSignal<File | null>(null);
	const [url, setUrl] = createSignal<string>("");

	const [uploadOptions, setUploadOptions] = createSignal({
		denoise: false,
		renderToFile: false,
		silenceCutoff: -40,
		minSkipDuration: 200
	});

	// createResource didn't work with booleans so I'm doing this manually.
	const [constants, setConstants] = createSignal<Constants | null>(null);
	let abortController = new AbortController();
	const fetchAndSetConstants = () =>
		fetchConstants(GLOBAL_STATE.premium[0](), abortController.signal)
			.then(setConstants)
			.catch(ex => {
				if (ex.name === "AbortError") return;
			});
	createEffect(() => {
		fetchAndSetConstants();
		const retryHandle = setInterval(() => {
			if (constants()) {
				clearInterval(retryHandle);
				return;
			}
			abortController.abort();
			abortController = new AbortController();
			fetchAndSetConstants();
		}, 3000);
	});

	const isAcceptableAtUrl = createResourceDebounced(
		url,
		async url => (url !== "" ? await isUrlAcceptableFile(url) : undefined),
		400
	);

	const [fileTooBig, setFileTooBig] = createSignal<boolean>(false);

	createEffect(() => {
		if (!constants()) return;
		const pickerFileTooBig = (selectedFile()?.size ?? 0) > constants()!.maxFileSize;
		const urlFileTooBig = isAcceptableAtUrl() === "too-big";
		setFileTooBig(pickerFileTooBig || urlFileTooBig);
	});

	const clearFile = () => {
		setSelectedFile(null);
		setFileTooBig(false);
	};

	const oneOfInputsIsValid = () => {
		return (selectedFile() && !fileTooBig()) || isAcceptableAtUrl() === "ok";
	};

	const [uploadProgress, setUploadProgress] = createSignal(0);

	const tryUpload = () => {
		setUploadProgress(0.01);
		const file = selectedFile();
		if (file) {
			const xhr = new XMLHttpRequest();
			xhr.open("POST", "http://localhost:3001/new-task/use-file", true);
			// xhr.setRequestHeader("Content-Type", "application/octet-stream");

			xhr.upload.onprogress = function (e) {
				if (e.lengthComputable) {
					const percentCompleted = Math.round((e.loaded * 100) / e.total);
					setUploadProgress(percentCompleted);
				}
			};

			xhr.onload = function () {
				if (xhr.status === 200) {
					console.log("Upload successful");
					navigate("/task/" + xhr.responseText);
				} else {
					console.log("Upload failed");
				}
				setUploadProgress(0);
			};

			xhr.onerror = function () {
				console.log("Upload failed");
				setUploadProgress(0);
			};

			file.arrayBuffer().then(buffer => xhr.send(buffer));
		}
	};
	let a = 0;
	const isUploading = () => uploadProgress() !== 0;

	return (
		<>
			<div id="upload-container">
				<div>
					<h5>Select a video from your device</h5>
					<SideBySide>
						<FileSelector
							accept=".mp4, video/webm"
							disabled={isUploading() || isAcceptableAtUrl() === "ok"}
							signal={[selectedFile, setSelectedFile]}
						/>
						{selectedFile() && (
							<Button disabled={isUploading()} onClick={clearFile}>
								Clear
							</Button>
						)}
					</SideBySide>
					<h5>Or paste a url</h5>
					<SideBySide>
						<InputField
							disabled={!!selectedFile()}
							type="text"
							placeholder="https://"
							signal={[url, setUrl]}
						/>
						{isAcceptableAtUrl() && (
							<span id="video-from-url-error">
								{mapIsUrlAcceptableFileOutputToMessage(isAcceptableAtUrl()!)}
							</span>
						)}
					</SideBySide>
					<h5>
						MP4, WebM are supported
						<br />
						Max file size:{" "}
						<span id="max-file-size" classList={{highlighted: fileTooBig()}}>
							{constants() === null ? "..." : constants()!.maxFileSize / 1024 / 1024 + " MiB"}
						</span>
					</h5>
				</div>
				<div id="submit-options">
					<h5>Options</h5>
					<RegularOptions signal={[uploadOptions, setUploadOptions]} constants={constants()} />
					<PremiumOnlyOptions signal={[uploadOptions, setUploadOptions]} />
				</div>
			</div>
			<Button disabled={!oneOfInputsIsValid() || isUploading()} variant="accent" onClick={tryUpload}>
				Upload
			</Button>
			<UploadProgressBar progress={uploadProgress()} show={isUploading()} />
		</>
	);
}

function SideBySide(props: {children: JSX.Element | Array<JSX.Element>}) {
	return <div class="side-by-side">{props.children}</div>;
}

function PremiumOnlyOptions<
	T extends {
		denoise: boolean;
		renderToFile: boolean;
	}
>(props: {signal: Signal<T>}) {
	return (
		<Fieldset legend="Premium only">
			<Switch
				small
				disabled={!GLOBAL_STATE.premium[0]()}
				value={props.signal[0]().denoise}
				onChange={v => props.signal[1](prev => ({...prev, denoise: v}))}
			>
				Denoise
				<InfoTooltip text="Process the sound of the video to recude background noise. Rendering to file is necessary in this case." />
			</Switch>
			<Switch
				small
				disabled={!GLOBAL_STATE.premium[0]() || props.signal[0]().denoise}
				value={props.signal[0]().renderToFile || props.signal[0]().denoise}
				onChange={v => props.signal[1](prev => ({...prev, renderToFile: v}))}
			>
				Render to file
				<InfoTooltip text="The processed video will be available for download." />
			</Switch>
		</Fieldset>
	);
}

function RegularOptions<
	T extends {
		silenceCutoff: number;
		minSkipDuration: number;
	}
>(props: {signal: Signal<T>; constants: Constants | null}) {
	const [signal, setSignal] = props.signal;

	const constants = () => props.constants;
	const silenceCutoffFmt = () => (constants() ? signal().silenceCutoff.toFixed(1) : "...") + "dB";
	const minSkipDurationFmt = () => (constants() ? signal().minSkipDuration : "...") + "ms";

	return (
		<>
			<Slider
				disabled={props.constants === null}
				// hideKnobOnDisabled
				lighter
				fillSpace
				// this value is put in the silenceremove ffmpeg filter
				// the more negative it is, the quieter the silence has to be to be removed
				// backend returns the more negative bound as min
				min={constants()?.silenceCutoff.min ?? -1}
				max={constants()?.silenceCutoff.max ?? 1}
				step={0.01}
				value={constants() ? signal().silenceCutoff : 0}
				onInput={v => setSignal(prev => ({...prev, silenceCutoff: +v.currentTarget.value}))}
			>
				Silence cutoff {silenceCutoffFmt()}
			</Slider>
			<Slider
				disabled={props.constants === null}
				// hideKnobOnDisabled
				lighter
				fillSpace
				min={constants()?.skipDuration.min ?? -1}
				max={constants()?.skipDuration.max ?? 1}
				step={1}
				value={constants() ? signal().minSkipDuration : 0}
				onInput={v => setSignal(prev => ({...prev, minSkipDuration: +v.currentTarget.value}))}
			>
				Min skip duration {minSkipDurationFmt()}
			</Slider>
		</>
	);
}

function Fieldset(props: {children: JSX.Element; legend: string}) {
	return (
		<fieldset class="rounded">
			<legend>{props.legend}</legend>
			{props.children}
		</fieldset>
	);
}

function UploadProgressBar(props: {progress: number; show: boolean}) {
	// delay showing the progress bar so the reverse show animation doesn't play when the page loads
	const [opacity, setOpacity] = createSignal(0);
	setTimeout(() => setOpacity(1), 600);
	return (
		<>
			<ProgressBar
				value={props.progress}
				class={props.show ? "progress-bar-main-show" : "progress-bar-main-hide"}
				style={{
					opacity: opacity(),
					"margin-top": "1em",
					"border-bottom-left-radius": 0,
					"border-bottom-right-radius": 0
				}}
			/>
			<div id="progress-bar-obscure" />
		</>
	);
}
