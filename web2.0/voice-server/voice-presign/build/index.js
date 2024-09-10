const crypto = require("crypto");
function sign(key, msg) {
  return crypto.createHmac("sha256", key).update(msg, "utf8").digest();
}
function getSignatureKey(secretKey, dateStamp, region, service) {
  const kDate = sign(Buffer.from("AWS4" + secretKey, "utf8"), dateStamp);
  const kRegion = sign(kDate, region);
  const kService = sign(kRegion, service);
  const kSigning = sign(kService, "aws4_request");
  return kSigning;
}
function generatePresignedUrl(accessKey, secretKey, bucketName, objectName, region, expiration) {
  const now = new Date;
  const amzDate = now.toISOString().replace(/[:\-]|\.\d{3}/g, "");
  const dateStamp = amzDate.slice(0, 8);
  const method = "PUT";
  const service = "s3";
  const algorithm = "AWS4-HMAC-SHA256";
  const host = `${bucketName}.storage.yandexcloud.net`;
  const canonicalUri = `/${objectName}`;
  const credentialScope = `${dateStamp}/${region}/${service}/aws4_request`;
  const canonicalQuerystring = `X-Amz-Algorithm=${algorithm}&X-Amz-Credential=${encodeURIComponent(accessKey + "/" + credentialScope)}&X-Amz-Date=${amzDate}&X-Amz-Expires=${expiration}&X-Amz-SignedHeaders=host`;
  const canonicalHeaders = `host:${host}\n`;
  const signedHeaders = "host";
  const payloadHash = "UNSIGNED-PAYLOAD";
  const canonicalRequest = `${method}\n${canonicalUri}\n${canonicalQuerystring}\n${canonicalHeaders}\n${signedHeaders}\n${payloadHash}`;
  const stringToSign = `${algorithm}\n${amzDate}\n${credentialScope}\n${crypto.createHash("sha256").update(canonicalRequest, "utf8").digest("hex")}`;
  const signingKey = getSignatureKey(secretKey, dateStamp, region, service);
  const signature = crypto.createHmac("sha256", signingKey).update(stringToSign, "utf8").digest("hex");
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
function isIntegrationRaw(request, context) {
  return request === undefined || request.headers === undefined || request.headers["X-Request-Id"] === undefined || request.headers["X-Request-Id"] !== context.requestId;
}
function main(request, context) {
  if (isIntegrationRaw(request, context)) {
    throw new Error(JSON.stringify({
      statusCode: 401,
      body: "`integration=raw` may not be used"
    }));
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
  } catch (ex) {
    return {
      statusCode: 500,
      body: `function is misconfigured: ${ex.toString()}`
    };
  }
  const ACCESS_KEY = process.env.ACCESS_KEY;
  const SECRET_KEY = process.env.SECRET_KEY;
  const BUCKET = process.env.BUCKET;
  const EXPIRY = +process.env.EXPIRY;
  const uniqueId = crypto.randomBytes(32).toString("hex");
  const filepath = `${uniqueId}/input`;
  return {
    statusCode: 200,
    body: generatePresignedUrl(ACCESS_KEY, SECRET_KEY, BUCKET, filepath, "ru-central1", EXPIRY)
  };
}
exports.main = main;
