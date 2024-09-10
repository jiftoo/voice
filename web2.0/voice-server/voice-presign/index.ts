const crypto: typeof import("crypto") = require("crypto");

// Helper function to sign the key
function sign(key: Buffer, msg: string): Buffer {
	return crypto.createHmac("sha256", key).update(msg, "utf8").digest();
}

// Generate signature key based on the secret key, date, region, and service
function getSignatureKey(secretKey: string, dateStamp: string, region: string, service: string): Buffer {
	const kDate = sign(Buffer.from("AWS4" + secretKey, "utf8"), dateStamp);
	const kRegion = sign(kDate, region);
	const kService = sign(kRegion, service);
	const kSigning = sign(kService, "aws4_request");
	return kSigning;
}

// Main function to generate presigned URL
function generatePresignedUrl(
	accessKey: string,
	secretKey: string,
	bucketName: string,
	objectName: string,
	region: string,
	expiration: number
): string {
	// Get current date and time
	const now = new Date();
	const amzDate = now.toISOString().replace(/[:\-]|\.\d{3}/g, ""); // YYYYMMDDTHHMMSSZ
	const dateStamp = amzDate.slice(0, 8); // YYYYMMDD

	// Set up values for signing
	const method = "PUT";
	const service = "s3";
	const algorithm = "AWS4-HMAC-SHA256";
	const host = `${bucketName}.storage.yandexcloud.net`;
	const canonicalUri = `/${objectName}`;
	const credentialScope = `${dateStamp}/${region}/${service}/aws4_request`;

	// Step 1: Create Canonical Request
	const canonicalQuerystring = `X-Amz-Algorithm=${algorithm}&X-Amz-Credential=${encodeURIComponent(
		accessKey + "/" + credentialScope
	)}&X-Amz-Date=${amzDate}&X-Amz-Expires=${expiration}&X-Amz-SignedHeaders=host`;
	const canonicalHeaders = `host:${host}\n`;
	const signedHeaders = "host";
	const payloadHash = "UNSIGNED-PAYLOAD";

	const canonicalRequest = `${method}\n${canonicalUri}\n${canonicalQuerystring}\n${canonicalHeaders}\n${signedHeaders}\n${payloadHash}`;

	// Step 2: Create the String to Sign
	const stringToSign = `${algorithm}\n${amzDate}\n${credentialScope}\n${crypto
		.createHash("sha256")
		.update(canonicalRequest, "utf8")
		.digest("hex")}`;

	// Step 3: Calculate the Signature
	const signingKey = getSignatureKey(secretKey, dateStamp, region, service);
	const signature = crypto.createHmac("sha256", signingKey).update(stringToSign, "utf8").digest("hex");

	// Step 4: Create the final presigned URL
	const presignedUrl = `https://${host}${canonicalUri}?${canonicalQuerystring}&X-Amz-Signature=${signature}`;

	return presignedUrl;
}

function checkVars() {
	const EXPIRY = process.env.EXPIRY;
	const ACCESS_KEY = process.env.ACCESS_KEY;
	const SECRET_KEY = process.env.SECRET_KEY;
	const BUCKET = process.env.BUCKET;

	if (EXPIRY === undefined) {
		throw new Error("EXPIRY not set");
	}
	if (isNaN(+EXPIRY)) {
		throw new Error("EXPIRY not a number");
	}

	if (ACCESS_KEY === undefined) {
		throw new Error("ACCESS_KEY not set");
	}

	if (SECRET_KEY === undefined) {
		throw new Error("SECRET_KEY not set");
	}

	if (BUCKET === undefined) {
		throw new Error("BUCKET not set");
	}
}

function isIntegrationRaw(request: any, context: any) {
	return (
		request === undefined ||
		request.headers === undefined ||
		request.headers["X-Request-Id"] === undefined ||
		request.headers["X-Request-Id"] !== context.requestId
	);
}

function main(request: {httpMethod: string}, context: any): {statusCode: number; headers?: any; body?: any} {
	if (isIntegrationRaw(request, context)) {
		throw new Error(
			JSON.stringify({
				statusCode: 401,
				body: "`integration=raw` may not be used"
			})
		);
	}
	console.log("http method", request.httpMethod);

	if (request.httpMethod !== "GET") {
		return {
			statusCode: 405,
			body: `\`${request.httpMethod}\` is not allowed`,
			headers: {
				allow: "GET"
			}
		};
	}

	try {
		checkVars();
	} catch (ex: any) {
		return {
			statusCode: 500,
			body: `function is misconfigured: ${ex.toString()}`
		};
	}

	const ACCESS_KEY = process.env.ACCESS_KEY!;
	const SECRET_KEY = process.env.SECRET_KEY!;
	const BUCKET = process.env.BUCKET!;
	const EXPIRY = +process.env.EXPIRY!;

	const uniqueId = crypto.randomBytes(32).toString("hex");
	const filepath = `${uniqueId}/input`;

	return {
		statusCode: 200,
		body: generatePresignedUrl(ACCESS_KEY, SECRET_KEY, BUCKET, filepath, "ru-central1", EXPIRY)
	};
}

exports.main = main;
