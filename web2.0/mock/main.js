/* eslint-disable */
const express = require("express");
const cors = require("cors");
const app = express();
const port = 3001;

app.use(express.text());
app.use(cors());

// not valid url => 422
// not valid video => 415
// too large => 413; message: <real size>
// valid => 200; message: <real size>
app.post("/check-upload-url", async (req, res) => {
	const check = async (req, res) => {
		try {
			let url = new URL(req.body);
			if (!url.protocol.startsWith("http")) {
				return res.sendStatus(422);
			}
		} catch (_) {
			return res.sendStatus(422);
		}
		const response = await fetch(req.body, {
			method: "HEAD"
		})
			.then(v => [v.headers.get("content-type"), v.headers.get("content-length")])
			.catch(() => {
				return null;
			});
		if (response === null) {
			return res.sendStatus(422);
		}
		if (!response[0].startsWith("video/")) {
			return res.sendStatus(415);
		}
		const maxFileSize = constants(req.query.premium === "true").maxFileSize;
		console.log("file size", response[1], maxFileSize);
		if (response[1] > maxFileSize) {
			return res.status(413).send(response[1]);
		}

		return res.status(200).send(response[1]);
	};
	const t1 = performance.now();
	const response = await check(req, res);
	console.log("checking url", req.body, "=>", response.statusCode, (performance.now() - t1).toFixed(2) + "ms");
	return response;
});

const constants = premium => {
	console.log("file size", premium);
	let maxFileSize;
	let skipDuration;
	if (premium) {
		// 100 MiB
		maxFileSize = 104857600;
		skipDuration = {min: 50, max: 500};
	} else {
		// 25 MiB
		// maxFileSize = 26214400;
		maxFileSize = 10000000;
		skipDuration = {min: 100, max: 250};
	}
	return {
		silenceCutoff: {min: -90, max: -10},
		skipDuration,
		maxFileSize
	};
};
app.get("/constants", (req, res) => {
	const premium = req.query.premium === "true" ?? false;
	console.log("file size", premium);
	res.header("Cache-Control", "no-cache");
	return res.send(constants(premium));
});

app.post("/new-task/use-file", (req, res) => {
	console.log("reading file", req.body.length);
	return res.send(Math.random().toString(36).substring(2, 15));
});

app.post("/new-task/use-url", (req, res) => {
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
