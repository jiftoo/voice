import {GLOBAL_STATE} from "./globalState";

const BACKEND_URL = "http://localhost:3001/";

const isPremium = () => GLOBAL_STATE.premium[0]();

export type NoError<T> = {data: T; error: null};
export type NetworkError = {data: null; error: {type: "network"; message: string}};
export type ClientError = {data: null; error: {type: "server"; statusCode: number; message?: string}};
export type RestResult<T> = NoError<T> | NetworkError | ClientError;

export type IsUrlAcceptableFileOutput = ["ok", number] | ["bad-url", null] | ["not-video", null] | ["too-big", number];

/**
 * Check if the given URL is acceptable for upload.
 */
export async function checkUploadUrl(url: string): Promise<RestResult<IsUrlAcceptableFileOutput>> {
	const [result, error] = await post("check-upload-url?premium=" + isPremium(), url, "text/plain", false)
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
				case 200:
					result = ["ok", +body!];
					break;
				case 422:
					result = ["bad-url", null];
					break;
				case 415:
					result = ["not-video", null];
					break;
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

function post(
	path: string,
	body: any,
	contentType: string,
	credentials: boolean,
	headers?: HeadersInit
): Promise<Response> {
	return fetch(BACKEND_URL + path, {
		method: "POST",
		headers: {
			"Content-Type": contentType,
			...headers
		},
		credentials: credentials ? "include" : undefined,
		body: body
	});
}
