import {JSX, Setter, Signal, createEffect, createResource, createSignal} from "solid-js";
import {useNavigate} from "@solidjs/router";
import "./Upload.css";
import {GLOBAL_STATE} from "../globalState";
import {Loading, createResourceDebounced} from "../util";
import Button from "../components/Button";
import FileSelector from "../components/FileSelector";
import InputField from "../components/InputField";
import ProgressBar from "../components/ProgressBar";
import Switch from "../components/Switch";
import Slider from "../components/Slider";
import InfoTooltip from "../components/InfoTooltip";
import {
	IsUrlAcceptableFileOutput,
	RestResult,
	checkUploadUrl,
	fetchConstants,
	newUrlOrNull,
	xhrUploadFile
} from "../rest";

function mapIsUrlAcceptableFileOutputToMessage(output: RestResult<IsUrlAcceptableFileOutput>): string {
	if (output.error) {
		if (output.error.type === "server") {
			return "Server error!";
		} else if (output.error.type === "network") {
			return "Request error or timeout!";
		}
	} else {
		const [status, msg] = output.data;
		switch (status) {
			case "ok":
				return "";
			case "bad-url":
				if (msg.validWithScheme) {
					return "Try adding https:// or http:// to the url!";
				}
				return "Url is not valid!";
			case "unreachable":
				return "Url is unreachable!";
			case "request-error":
				return "Request did not succeed!";
			case "not-video":
				return "Url is not a valid video!";
			case "bad-response":
				return "Endpoint doesn't share content-length!";
			case "too-big":
				return "Video is too big!";
		}
	}
	return "unreachable";
}

type Constants = {
	silenceCutoff: {min: number; max: number};
	skipDuration: {min: number; max: number};
	maxFileSize: number;
};

async function test() {
	console.log("https://google.com", await checkUploadUrl("https://google.com"));
	console.log("file://sex", await checkUploadUrl("file://sex"));
	console.log("https://i.redd.it/ruplztk6xf9c1.png", await checkUploadUrl("https://i.redd.it/ruplztk6xf9c1.png"));
	console.log("1234", await checkUploadUrl("1234"));
	console.log(
		"https://cdn.discordapp.com/attachments/926206953976377374/1190604987638747207/Hardtek_Fake_to_Fake_dat_file_records_jq7_uPeMjIE.mp4",
		await checkUploadUrl(
			"https://cdn.discordapp.com/attachments/926206953976377374/1190604987638747207/Hardtek_Fake_to_Fake_dat_file_records_jq7_uPeMjIE.mp4"
		)
	);
}

export default function Upload() {
	// test();
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
		// retry once
		setTimeout(() => {
			if (!constants()) {
				fetchAndSetConstants();
			}
		}, 5000);
	});

	const [isAcceptableAtUrl, {loading: isAcceptableAtUrlLoading}] = createResourceDebounced(
		url,
		async url => (url !== "" ? await checkUploadUrl(url) : undefined),
		400
	);

	createEffect(() => {
		console.log("canTryUpload", canTryUpload());
	});

	const [pickedFileSize, setPickedFileSize] = createSignal<number | null>(null);
	const fileTooBig = () =>
		constants() !== null && pickedFileSize() !== null ? pickedFileSize()! > constants()!.maxFileSize : false;
	// (selectedFile()?.size ?? 0) > constants()!.maxFileSize || isAcceptableAtUrl()?.data === "too-big";

	createEffect(() => {
		if (!constants()) return;
		const urlData = isAcceptableAtUrl()?.data;
		if (urlData && (urlData[0] === "ok" || urlData[0] === "too-big")) {
			setPickedFileSize(urlData[1]);
		} else {
			setPickedFileSize(selectedFile()?.size ?? null);
		}
	});

	const clearFile = () => {
		setSelectedFile(null);
		setPickedFileSize(null);
	};

	const oneOfInputsIsValid = () => {
		return Boolean(
			(selectedFile() && !fileTooBig()) || (isAcceptableAtUrl()?.data && isAcceptableAtUrl()?.data?.[0] === "ok")
		);
	};

	const canTryUpload = () => oneOfInputsIsValid() && !isUploading();

	const [uploadProgress, setUploadProgress] = createSignal(0);

	const tryUpload = () => {
		setUploadProgress(0.01);
		let dataToSend: File | URL | null = null;
		if (selectedFile()) {
			dataToSend = selectedFile();
		} else if (isAcceptableAtUrl()?.data?.[0] === "ok") {
			dataToSend = newUrlOrNull(url());
		}

		console.log("tryUpload data", dataToSend);
		if (dataToSend !== null) {
			xhrUploadFile(
				dataToSend,
				progress => {
					// preserve the magic value which makes the progress bar show up
					setUploadProgress(Math.max(progress, 0.01));
				},
				(status, responseText) => {
					if (status === 200) {
						console.log("Upload successful");
						navigate("/task/" + responseText);
					} else {
						console.log("Upload failed");
					}
					setUploadProgress(0);
				},
				() => {
					console.log("Upload failed");
					setUploadProgress(0);
				}
			);
		}
	};

	const isUploading = () => uploadProgress() !== 0;

	return (
		<>
			<div id="upload-container">
				<div
					onKeyDown={ev => {
						if (ev.key === "Enter" && canTryUpload()) {
							tryUpload();
						}
					}}
				>
					<h5>Select a video from your device</h5>
					<SideBySide>
						<FileSelector
							accept=".mp4, video/webm"
							disabled={isUploading() || isAcceptableAtUrl()?.data?.[0] === "ok"}
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
						{isAcceptableAtUrlLoading() && <Loading />}
						{isAcceptableAtUrl() && !isAcceptableAtUrlLoading() && (
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
							{constants() === null
								? "..."
								: (constants()!.maxFileSize / 1024 / 1024).toFixed(2) + " MiB"}
						</span>
					</h5>
				</div>
				<div id="submit-options">
					<h5>Options</h5>
					<RegularOptions signal={[uploadOptions, setUploadOptions]} constants={constants()} />
					<PremiumOnlyOptions signal={[uploadOptions, setUploadOptions]} />
				</div>
			</div>
			<Button disabled={!canTryUpload()} variant="accent" onClick={tryUpload}>
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
	const constants = () => props.constants;
	const silenceCutoffFmt = () => (constants() ? props.signal[0]().silenceCutoff.toFixed(0) : "...") + "dB";
	const minSkipDurationFmt = () => (constants() ? props.signal[0]().minSkipDuration : "...") + "ms";

	return (
		<>
			<div class="info-cirlce-flex">
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
					value={constants() ? props.signal[0]().silenceCutoff : 0}
					onInput={v => props.signal[1](prev => ({...prev, silenceCutoff: +v.currentTarget.value}))}
				>
					<span class="smaller-letter-spacing">Silence cutoff</span> {silenceCutoffFmt()}{" "}
				</Slider>
				<InfoTooltip text="The more negative this value is, the quieter the silence has to be to be removed." />
			</div>
			<div class="info-cirlce-flex">
				<Slider
					disabled={props.constants === null}
					// hideKnobOnDisabled
					lighter
					fillSpace
					min={constants()?.skipDuration.min ?? -1}
					max={constants()?.skipDuration.max ?? 1}
					step={1}
					value={constants() ? props.signal[0]().minSkipDuration : 0}
					onInput={v => props.signal[1](prev => ({...prev, minSkipDuration: +v.currentTarget.value}))}
				>
					<span class="smaller-letter-spacing">Min skip duration</span> {minSkipDurationFmt()}
				</Slider>
				<InfoTooltip
					text={
						"Higher values improve user experience by accounting for browser delay in skipping short periods of silence.\nRendering to a video file does not suffer from this."
					}
				/>
			</div>
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
