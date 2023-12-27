/* eslint-disable */
const express = require("express");
const app = express();
const port = 3001;

app.use(express.text());

app.post("/check-upload-url", (req, res) => {
	res.header("Access-Control-Allow-Origin", "*");
	console.log("body", req.body);
	if (req.body == "415") {
		return res.sendStatus(415);
	}
	if (req.body == "413") {
		return res.sendStatus(413);
	}
	if (req.body == "500") {
		return res.sendStatus(500);
	}
	return res.sendStatus(200);
});

app.get("/constants", (req, res) => {
	const premium = req.query.premium === "true" ?? false;
	console.log("file size", premium);
	res.header("Access-Control-Allow-Origin", "*");
	res.header("Cache-Control", "no-cache");
	let maxFileSize;
	let skipDuration;
	if (premium) {
		// 100 MiB
		maxFileSize = 104857600;
		skipDuration = {min: 50, max: 500};
	} else {
		// 25 MiB
		maxFileSize = 26214400;
		skipDuration = {min: 100, max: 250};
	}
	return res.send({
		silenceCutoff: {min: -90, max: -10},
		skipDuration,
		maxFileSize
	});
});

app.options("/new-task/use-file", (req, res) => {
	res.header("Access-Control-Allow-Origin", "*");
	return res.sendStatus(200);
});
app.post("/new-task/use-file", (req, res) => {
	res.header("Access-Control-Allow-Origin", "*");
	console.log("reading file", req.body.length);
	return res.send(Math.random().toString(36).substring(2, 15));
});

app.options("/new-task/use-url", (req, res) => {
	res.header("Access-Control-Allow-Origin", "*");
	return res.sendStatus(200);
});
app.post("/new-task/use-url", (req, res) => {
	res.header("Access-Control-Allow-Origin", "*");
	if (req.body.length > 1000) {
		console.log("reading url", "body too long");
	} else {
		console.log("reading url", req.body);
	}
	return res.send(Math.random().toString(36).substring(2, 15));
});

app.listen(port, () => {
	console.log(`Listening on port ${port}`);
});
