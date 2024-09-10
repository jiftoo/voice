const response = await fetch("https://functions.yandexcloud.net/d4egr7s2q2m3v4g3v8ee", {
	method: "GET"
});

console.log(response.status, response.headers);

const url = new URL(await response.text());
console.log("url", url.toString());

{
	console.log("sending video");
	const response = await fetch(url, {
		method: "PUT",
		body: Bun.file("prim.mp4")
	});

	console.log(response.status, await response.text());
}
