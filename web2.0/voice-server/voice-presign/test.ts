const response = await fetch("https://functions.yandexcloud.net/d4egr7s2q2m3v4g3v8ee", {
	method: "GET"
});

console.log(response.status, response.headers, await response.text());
