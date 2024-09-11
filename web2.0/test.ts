// const response = await fetch("https://bba44pv1r6rdqsguq4r5.containers.yandexcloud.net/check-upload-url?premium=true", {
// 	method: "PUT",
// 	body: "https://litter.catbox.moe/gco6ta.mp4"
// 	// body: "https://litter.catbox.moe/5wzci7.mp4"
// 	// body: "https://litter.catbox.moe/h7h5p4.gif"
// });

// console.log(response.status, response.statusText, response.headers, await response.text());

const PRESIGN_URL_BACKEND_URL = "https://functions.yandexcloud.net/d4egr7s2q2m3v4g3v8ee/";

const presigned = await fetch(PRESIGN_URL_BACKEND_URL).then(v => v.text());

console.log(presigned)
const response = await fetch(presigned, {
	method: "PUT",
	headers: {
		Host: "voice-upload-bucker-69.storage.yandexcloud.net",
		"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:131.0) Gecko/20100101 Firefox/131.0",
		Accept: "*/*",
		"Accept-Language": "en-GB,en;q=0.5",
		"Accept-Encoding": "gzip, deflate, br, zstd",
		"Access-Control-Request-Method": "PUT",
		"Access-Control-Request-Headers": "content-type",
		Referer: "https://localhost:3000/",
		Origin: "https://localhost:3000",
		DNT: "1",
		Connection: "keep-alive",
		"Sec-Fetch-Dest": "empty",
		"Sec-Fetch-Mode": "cors",
		"Sec-Fetch-Site": "cross-site",
		Priority: "u=4",
		TE: "trailers"
	},
});

console.log(response.status,response.headers, await response.text());
