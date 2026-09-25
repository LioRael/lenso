import assert from "node:assert/strict";

const base = new URL(process.env.LENSO_HTTP_APP_URL ?? process.env.LENSO_WORKERS_APP_URL ?? "http://127.0.0.1:63739");
const environment = process.env.LENSO_HTTP_APP_ENVIRONMENT ?? "local-workerd";
const corpus = process.env.LENSO_HTTP_CORPUS ?? "full";
if (!["local-workerd", "native"].includes(environment) ||
    !["full", "shared"].includes(corpus) || (environment === "native" && corpus !== "shared")) {
  throw new Error("select local-workerd/full or either environment/shared HTTP corpus");
}
if (base.protocol !== "http:" || base.hostname !== "127.0.0.1" || !base.port ||
    base.pathname !== "/" || base.search || base.hash || base.username || base.password) {
  throw new Error("HTTP App URL must be an exact loopback HTTP origin");
}

const cases = [
  ["method", "GET", "/method/42", [], 200, Buffer.from("GET /method/42")],
  ["binary", "POST", "/bytes", [0, 255, 128, 13, 10, 1], 200, Buffer.from([0, 255, 128, 13, 10, 1])],
  ["evidence", "GET", "/evidence", [], 200, Buffer.from("bearer:alpha"),
    { "x-test": "alpha", authorization: "Bearer token" }],
  ["method_not_allowed", "POST", "/method/42", [], 405, Buffer.from('{"error":"method_not_allowed"}')],
  ["not_found", "GET", "/absent", [], 404, Buffer.from('{"error":"not_found"}')],
  ["encoded_path", "GET", "/method/%2F", [], 400, Buffer.from('{"error":"unsupported_path_encoding"}')],
  ["domain_error", "GET", "/reject", [], 502, Buffer.from('{"error":"endpoint_rejected"}')],
  ["runtime_failure", "GET", "/failure", [], 503, Buffer.from('{"error":"endpoint_unavailable"}')],
  ["oversized_body", "POST", "/bytes", Array(65_537).fill(1), 413, Buffer.from('{"error":"request_too_large"}')],
  ["healthy_after_failure", "GET", "/method/42", [], 200, Buffer.from("GET /method/42")],
];

const results = [];
const sharedNames = new Set(["method", "binary", "evidence", "method_not_allowed", "not_found", "domain_error", "runtime_failure"]);
for (const [name, method, path, body, status, expectedBody, headers = {}] of
  cases.filter(([name]) => corpus === "full" || sharedNames.has(name))) {
  try {
    const response = await fetch(new URL(path, base), {
      method,
      headers,
      ...(body.length ? { body: Buffer.from(body) } : {}),
    });
    const actualBody = Buffer.from(await response.arrayBuffer());
    const failures = [];
    if (response.status !== status) failures.push(`status ${response.status} != ${status}`);
    if (!actualBody.equals(expectedBody)) failures.push(`body ${actualBody.toString("utf8")} differs`);
    if (!response.headers.get("x-request-id")) failures.push("request ID missing");
    results.push({ name, passed: failures.length === 0, ...(failures.length ? { failures } : {}) });
  } catch (error) {
    results.push({ name, passed: false, error: String(error) });
  }
}
const passed = results.every((result) => result.passed);
process.stdout.write(`${JSON.stringify({ passed, scope: `verified V4 Bundle App build in ${environment}, ${corpus} HTTP corpus`, results }, null, 2)}\n`);
if (!passed) process.exitCode = 1;
