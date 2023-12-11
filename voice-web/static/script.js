function fetchApi(apiRoute, fetchOptions) {
	if (!apiRoute.startsWith("/")) {
		apiRoute = "/" + apiRoute;
	}
	return fetch("http://localhost:3000/api" + apiRoute, fetchOptions);
}

function getApi(apiRoute) {
	return fetchApi(apiRoute, {
		method: "GET",
		credentials: "include",
	}).catch((ex) => {
		writeLog("Failed to GET ${apiRoute}:", "error");
		writeLog(ex, "error");
	});
}

function postApi(apiRoute, body) {
	return fetchApi(apiRoute, {
		method: "POST",
		credentials: "include",
		body,
	}).catch((ex) => {
		writeLog("Failed to POST ${apiRoute}:", "error");
		writeLog(ex, "error");
	});
}

// ---------------------------------------------------------------------

function writeLog(message, level) {
	if (level != "info" && level != "warn" && level != "error") {
		level = "info";
		console.warn("invalid log level:", level);
	}

	const node = document.createElement("div");
	node.classList.add(level);
	node.textContent = typeof message == "string" ? message : JSON.stringify(message, null, 2);
	document.getElementById("log-data").appendChild(node);

	console[level](message);
}

// ---------------------------------------------------------------------

async function queryLargestAllowedFileSize() {
	let size = await getApi("max-file-size");
	if (typeof size !== "number") {
		// 64 megabytes fallback
		const fallback = 6.4e7;
		writeLog(`Invalid max file size: ${JSON.stringify(size)}. Falling back to ${fallback} bytes`, "error");
		return fallback;
	}
	return size;
}

async function verifySelectedFile(ev) {
	const file = ev.target.files[0];
	if (!file) {
		return false;
	}

	const max = await queryLargestAllowedFileSize();

	if (file.size > max) {
		writeLog(`File too big (${file.size} bytes). Maximum allowed is ${max} bytes`, "error");
		return false;
	}

	if (!file.type.includes("audio") && !file.type.includes("video")) {
		writeLog(`File format is not supported: ${file.type}`, "error");
		return false;
	}

	return true;
}

function updateSelectedFileName(name) {
	document.getElementById("selected-file-name").innerText = name;
}

// ---------------------------------------------------------------------

document.getElementById("hidden-file-input").addEventListener("change", async (ev) => {
	console.log(ev.target.files);
	if (!(await verifySelectedFile(ev))) {
		console.error("invalid file");
		return;
	}

	const file = ev.target.files[0];

	updateSelectedFileName(file.name);
});

document.getElementById("select-file-button").addEventListener("click", () => {
	document.getElementById("hidden-file-input").click();
});

document.querySelectorAll("label input[type='range']").forEach((v) => {
	if (v.nextElementSibling.classList.contains("slider-value")) {
		let valSpan = v.nextElementSibling;
		const prefix = valSpan.dataset.prefix ?? "";
		const suffix = valSpan.dataset.suffix ?? "";
		v.addEventListener("input", (ev) => {
			valSpan.innerText = `${prefix} ${ev.target.value} ${suffix}`;
		});
		v.dispatchEvent(new Event("input"));
	} else {
		console.log("range input without slider-value sibling", v);
	}
});
