import {GLOBAL_STATE} from "./globalState";

const PRESIGN_URL_BACKEND_URL = "https://functions.yandexcloud.net/d4egr7s2q2m3v4g3v8ee/";
const BUCKET_URL = "https://voice-upload-bucker-69.storage.yandexcloud.net/"; // TODO: something more robust
const URL_UPLOAD_BACKEND_URL = "https://bba44pv1r6rdqsguq4r5.containers.yandexcloud.net/";
const WAVEFORM_BACKEND_URL = "https://bbaj25vmu8ilqvlc6nat.containers.yandexcloud.net/";
const ANALYZE_BACKEND_URL = "https://bbajrpitqs7cp4hvcgal.containers.yandexcloud.net/";

const isPremium = () => GLOBAL_STATE.premium[0]();

export type NoError<T> = {data: T; error: null};
export type NetworkError = {data: null; error: {type: "network"; message: string}};
export type ClientError = {data: null; error: {type: "server"; statusCode: number; message?: string}};
export type RestResult<T> = NoError<T> | NetworkError | ClientError;

export type IsUrlAcceptableFileOutput =
	| ["ok", number] // The url is acceptable for upload.
	| ["bad-url", {validWithScheme: boolean}] // The url is not a valid url.
	// validWithScheme is true if the url becomes well-formed when "http://" is prepended.
	// This improves the user experience for users who are not familiar with the concept of a url scheme.
	| ["unreachable", null] // The request to the url timed out.
	| ["request-error", null] // The request did not return 200 OK.
	| ["not-video", null] // The request was successful, but the response mime wasn't a video.
	| ["bad-response", null] // The request was successful, but the response did not prove that the video file is acceptable.
	| ["too-big", number]; // The request was successful, but the file was too big.

/**
 * Constuct a new URL object from the given string, or return null if the string is not a valid url.
 */
export function newUrlOrNull(url: string): URL | null {
	try {
		return new URL(url);
	} catch (e) {
		return null;
	}
}

/**
 * Check if the given URL is acceptable for upload.
 */
export async function checkUploadUrl(url: string): Promise<RestResult<IsUrlAcceptableFileOutput>> {
	// nodejs mock or the prod rust server.
	// todo: update the mock to include new status codes.
	const fetchCheckUploadUrl = async () => {
		return await put(URL_UPLOAD_BACKEND_URL, "check-upload-url?premium=" + isPremium(), url, "text/plain", false);
	};

	// check if the url is well-formed locally (it's still checked on the server, but we don't want to waste traffic).
	const urlObject = newUrlOrNull(url);
	if (urlObject === null) {
		const urlObjectWithScheme = newUrlOrNull("http://" + url);
		// we want to only set this flag if the url actually looks like a url without a scheme.
		// e.g "example", "example." and "example.com" would trigger this. we want to skip the first two.
		const likelyUserError = urlObjectWithScheme?.hostname.split(".").every(v => v.length > 0) ?? false;
		return {
			data: ["bad-url", {validWithScheme: likelyUserError}],
			error: null
		};
	}

	const [result, error] = await fetchCheckUploadUrl()
		.then(v =>
			v
				.text()
				.then(body => [v, body] as const)
				.catch(() => [v, null] as const)
		)
		.then(([res, body]) => {
			let result: IsUrlAcceptableFileOutput | null = null;
			let error = null;
			switch (res.status) {
				// OK
				case 200:
					result = ["ok", +body!];
					break;
				// BAD_REQUEST
				case 400:
					result = ["bad-url", {validWithScheme: false}]; // scheme already checked by now
					break;
				// GATEWAY_TIMEOUT
				case 504:
					result = ["unreachable", null];
					break;
				// FAILED_DEPENDENCY
				case 424:
					result = ["request-error", null];
					break;
				// UNSUPPORTED_MEDIA_TYPE
				case 415:
					result = ["not-video", null];
					break;
				// UNPROCESSABLE_ENTITY
				case 422:
					result = ["bad-response", null];
					break;
				// PAYLOAD_TOO_LARGE
				case 413:
					result = ["too-big", +body!];
					break;
				default:
					error = {type: "server" as const, statusCode: res.status};
					break;
			}
			return [result, error] as const;
		})
		.catch(err => {
			return [null, {type: "network", message: err.toString()}] as const;
		});
	if (error) {
		return {
			data: null,
			error
		};
	}
	return {data: result!, error: null};
}

export async function fetchConstants(isPremium: boolean, abortSignal?: AbortSignal): Promise<unknown> {
	const res = await get(URL_UPLOAD_BACKEND_URL, "constants?premium=" + isPremium, false, undefined, abortSignal);
	return await res.json();
}

export type Uploadable = URL | File;

export async function xhrUploadFile(
	file: Uploadable,
	// progress is [0,1]
	onProgress: (progress: number) => void,
	onLoad: (status: number, fileId: string) => void,
	onError: () => void
) {
	if (file instanceof File) {
		return xhrUploadToS3(file, onProgress, onLoad, onError);
	}

	// or call upload-file to upload by URL

	const response = await fetch(URL_UPLOAD_BACKEND_URL, {
		method: "POST",
		body: (file as URL).toString()
	});

	onProgress(0.5);

	if (!response.ok) {
		console.log("error response when uploading by url", response.status, await response.text());
		onError();
		return;
	}

	onLoad(response.status, await response.text());
}

async function xhrUploadToS3(
	file: File,
	onProgress: (progress: number) => void,
	onLoad: (status: number, responseText: string) => void,
	onError: () => void
) {
	let presigned;
	try {
		presigned = await fetch(PRESIGN_URL_BACKEND_URL).then(v => v.text());
	} catch (ex) {
		console.error("exception while getting presigned url", ex);
		onError();
		return;
	}

	const pathname = new URL(presigned).pathname; // /foobar/input
	const fileId = pathname.split("/")[1];

	const xhr = new XMLHttpRequest();

	xhr.open("PUT", presigned, true);
	// xhr.setRequestHeader("Content-Type", file instanceof File ? "application/octet-stream" : "text/x-url");

	xhr.upload.onprogress = e => {
		let percentCompleted = e.lengthComputable ? Math.round((e.loaded * 100) / e.total) / 100 : 1.0;
		onProgress(percentCompleted);
	};

	xhr.onload = () => {
		onLoad(xhr.status, fileId);
	};
	xhr.onerror = onError as any;

	xhr.send(file);
}

export async function fetchSkips(videoId: string): Promise<Array<[number, number]>> {
	const response = await get(ANALYZE_BACKEND_URL, "analyze/" + videoId, false);
	return await response.json();
}

export function getWaveformEndpoint(videoId: string): string {
	return WAVEFORM_BACKEND_URL + videoId;
}

export function getReadFileEndpoint(videoId: string): string {
	return BUCKET_URL + videoId + "/input";
}

export function getAnalyzeEndpoint(videoId: string): string {
	return ANALYZE_BACKEND_URL + "analyze/" + videoId;
}

// -------------------------- fetch wrappers --------------------------

function get(
	backendUrl: string,
	path: string,
	credentials: boolean,
	headers?: HeadersInit,
	abortSignal?: AbortSignal
): Promise<Response> {
	return fetch(backendUrl + path, {
		method: "GET",
		headers: {
			...headers
		},
		credentials: credentials ? "include" : undefined,
		signal: abortSignal
	});
}

function post(
	backendUrl: string,
	path: string,
	body: any,
	contentType: string,
	credentials: boolean,
	headers?: HeadersInit
): Promise<Response> {
	return fetch(backendUrl + path, {
		method: "POST",
		headers: {
			"Content-Type": contentType,
			...headers
		},
		credentials: credentials ? "include" : undefined,
		body: body
	});
}

function put(
	backendUrl: string,
	path: string,
	body: any,
	contentType: string,
	credentials: boolean,
	headers?: HeadersInit
): Promise<Response> {
	return fetch(backendUrl + path, {
		method: "PUT",
		headers: {
			"Content-Type": contentType,
			...headers
		},
		credentials: credentials ? "include" : undefined,
		body: body
	});
}
