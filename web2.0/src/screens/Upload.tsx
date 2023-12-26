import {JSX, createEffect, createSignal} from "solid-js";
import {useNavigate} from "@solidjs/router";
import "./Upload.css";
import {GLOBAL_STATE} from "../globalState";
import {createResourceDebounced} from "../util";
import Button from "../components/Button";
import FileSelector from "../components/FileSelector";
import InputField from "../components/InputField";
import ProgressBar from "../components/ProgressBar";

type IsUrlAcceptableFileOutput = "ok" | "not-video" | "too-big" | "server-error" | "request-error";

async function isUrlAcceptableFile(url: string): Promise<IsUrlAcceptableFileOutput> {
	return fetch("http://localhost:3001/check-upload-url?premium=" + GLOBAL_STATE.premium[0](), {
		method: "POST",
		headers: {
			"Content-Type": "text/plain",
		},
		body: url,
	})
		.then((res) => {
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
		.catch((err) => {
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

function fetchMaxFileSize(isPremium: boolean): Promise<number> {
	return fetch("http://localhost:3001/max-file-size?premium=" + isPremium)
		.then((res) => res.text())
		.then((text) => parseInt(text));
}

export default function Upload() {
	const navigate = useNavigate();

	const [selectedFile, setSelectedFile] = createSignal<File | null>(null);
	const [url, setUrl] = createSignal<string>("");

	// createResource didn't work with booleans so I'm doing this manually.
	const [maxFileSize, setMaxFileSize] = createSignal(null);
	createEffect(() => fetchMaxFileSize(GLOBAL_STATE.premium[0]()).then(setMaxFileSize));

	const isAcceptableAtUrl = createResourceDebounced(url, async (url) => (url !== "" ? await isUrlAcceptableFile(url) : undefined), 400);

	const [fileTooBig, setFileTooBig] = createSignal<boolean>(false);

	createEffect(() => {
		const pickerFileTooBig = (selectedFile()?.size ?? 0) > maxFileSize()!;
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

			file.arrayBuffer().then((buffer) => xhr.send(buffer));
		}
	};

	const isUploading = () => uploadProgress() !== 0;

	return (
		<>
			<h5>Select a video from your device</h5>
			<SideBySide>
				<FileSelector accept=".mp4, video/webm" disabled={isUploading() || isAcceptableAtUrl() === "ok"} signal={[selectedFile, setSelectedFile]} />
				{selectedFile() && (
					<Button disabled={isUploading()} onClick={clearFile}>
						Clear
					</Button>
				)}
			</SideBySide>
			<h5>Or paste a url</h5>
			<SideBySide>
				<InputField disabled={!!selectedFile()} type="text" placeholder="https://" signal={[url, setUrl]} />
				{isAcceptableAtUrl() && <span id="video-from-url-error">{mapIsUrlAcceptableFileOutputToMessage(isAcceptableAtUrl()!)}</span>}
			</SideBySide>
			<h5>
				MP4, WebM are supported
				<br />
				Max file size:{" "}
				<span id="max-file-size" classList={{highlighted: fileTooBig()}}>
					{maxFileSize() === null ? "..." : maxFileSize()! / 1024 / 1024 + " MiB"}
				</span>
			</h5>
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

function UploadProgressBar(props: {progress: number; show: boolean}) {
	// delay showing the progress bar so the reverse show animation doesn't play when the page loads
	const [opacity, setOpacity] = createSignal(0);
	setTimeout(() => setOpacity(1), 600);
	return (
		<>
			<ProgressBar
				value={props.progress}
				class={props.show ? "progress-bar-main-show" : "progress-bar-main-hide"}
				style={{opacity: opacity(), "margin-top": "1em", "border-bottom-left-radius": 0, "border-bottom-right-radius": 0}}
			/>
			<div id="progress-bar-obscure" />
		</>
	);
}
