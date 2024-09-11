const response = await fetch("https://bba44pv1r6rdqsguq4r5.containers.yandexcloud.net/check-upload-url?premium=true", {
	method: "PUT",
	body: "https://litter.catbox.moe/gco6ta.mp4"
	// body: "https://litter.catbox.moe/5wzci7.mp4"
	// body: "https://litter.catbox.moe/h7h5p4.gif"
});

console.log(response.status, response.statusText, response.headers, await response.text());
