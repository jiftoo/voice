const response = await fetch("https://bba1rl7g84tdqpqgb68g.containers.yandexcloud.net/", {
	method: "POST",
});

console.log(response.status, response.headers);

const url = new URL(await response.text());

