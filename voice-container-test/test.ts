// const response = await fetch("https://bba1rl7g84tdqpqgb68g.containers.yandexcloud.net/", {
const response = await fetch("http://localhost", {
	method: "POST",
	body: null,
});

console.log(response.status, response.headers, await response.text());
