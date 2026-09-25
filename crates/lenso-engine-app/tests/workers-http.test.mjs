import assert from "node:assert/strict";
import { test } from "node:test";
import { createWorkersHttpHandler } from "../assets/workers-http.mjs";

const routes = [
  { route_id: "item", method: "GET", path: "/items/{id}" },
  { route_id: "bytes", method: "POST", path: "/bytes" },
  { route_id: "reject", method: "GET", path: "/reject" },
  { route_id: "failure", method: "GET", path: "/failure" },
];

function handler(calls = []) {
  return createWorkersHttpHandler({
    invoke(capability, operation, requestJson) {
      assert.equal(capability, "lenso.http.endpoint@1");
      if (operation === "describe") return JSON.stringify({ routes });
      const request = JSON.parse(requestJson);
      calls.push(request);
      if (request.route_id === "reject") throw { payload: '"rejected"' };
      if (request.route_id === "failure") throw new Error("guest failure");
      return JSON.stringify({
        status: 200,
        headers: [{ name: "content-type", value: "application/octet-stream" }],
        body: request.body,
      });
    },
  });
}

test("routes a Plan-admitted Endpoint with complete typed request and binary response", async () => {
  const calls = [];
  const fetch = handler(calls);
  const bytes = Uint8Array.from([0, 255, 128, 13, 10]);
  const response = await fetch(new Request("http://127.0.0.1/bytes?mode=raw", {
    method: "POST",
    headers: { authorization: "Bearer token", "x-test": "alpha" },
    body: bytes,
  }));
  assert.equal(response.status, 200);
  assert.deepEqual(new Uint8Array(await response.arrayBuffer()), bytes);
  assert.equal(response.headers.get("content-type"), "application/octet-stream");
  assert.equal(calls.length, 1);
  assert.equal(calls[0].route_id, "bytes");
  assert.equal(calls[0].method, "POST");
  assert.equal(calls[0].path, "/bytes");
  assert.equal(calls[0].query, "mode=raw");
  assert.deepEqual(calls[0].credential, { scheme: "bearer", value: "token" });
  assert.deepEqual(calls[0].path_parameters, []);
  assert.deepEqual(calls[0].headers, [{ name: "x-test", value: "alpha" }]);
  assert.equal(calls[0].request_id, response.headers.get("x-request-id"));
  assert.equal(calls[0].body, "AP+ADQo=");

  const parameterResponse = await fetch(new Request("http://127.0.0.1/items/42"));
  assert.equal(parameterResponse.status, 200);
  assert.deepEqual(calls[1].path_parameters, [{ name: "id", value: "42" }]);
});

test("rejects unsupported transport, ambiguous credential and oversize body", async () => {
  const calls = [];
  const fetch = handler(calls);
  for (const [request, status] of [
    [new Request("http://127.0.0.1/items/42", { headers: { cookie: "session=token" } }), 400],
    [new Request("http://127.0.0.1/items/42", { headers: { authorization: "Bearer first, second" } }), 400],
    [new Request("http://127.0.0.1/bytes", { method: "POST", body: new Uint8Array(65_537) }), 413],
  ]) {
    assert.equal((await fetch(request)).status, status);
  }
  assert.equal(calls.length, 0);
});

test("keeps route, domain, runtime and malformed response failures separate", async () => {
  const fetch = handler();
  assert.equal((await fetch(new Request("http://127.0.0.1/absent"))).status, 404);
  assert.equal((await fetch(new Request("http://127.0.0.1/items/%2F"))).status, 400);
  assert.equal((await fetch(new Request("http://127.0.0.1/items/42", { method: "POST" }))).status, 405);
  assert.equal((await fetch(new Request("http://127.0.0.1/reject"))).status, 502);
  assert.equal((await fetch(new Request("http://127.0.0.1/failure"))).status, 503);
  const malformed = createWorkersHttpHandler({
    invoke(_capability, operation) {
      return operation === "describe"
        ? JSON.stringify({ routes })
        : JSON.stringify({ status: 200, headers: [{ name: "connection", value: "close" }], body: "" });
    },
  });
  const result = await malformed(new Request("http://127.0.0.1/items/42"));
  assert.equal(result.status, 502);
  assert.deepEqual(await result.json(), { error: "invalid_endpoint_response" });
  const oversizedHeader = createWorkersHttpHandler({
    invoke(_capability, operation) {
      return operation === "describe"
        ? JSON.stringify({ routes })
        : JSON.stringify({ status: 200, headers: [{ name: "x-large", value: "x".repeat(150_000) }], body: "" });
    },
  });
  assert.equal((await oversizedHeader(new Request("http://127.0.0.1/items/42"))).status, 502);
  const unsupportedCookie = createWorkersHttpHandler({
    invoke(_capability, operation) {
      return operation === "describe"
        ? JSON.stringify({ routes })
        : JSON.stringify({ status: 200, headers: [{ name: "set-cookie", value: "session=abc; Path=/" }], body: "" });
    },
  });
  assert.equal((await unsupportedCookie(new Request("http://127.0.0.1/items/42"))).status, 502);
});

test("fails closed on overlapping or unsupported Guest route descriptions", () => {
  for (const declared of [
    [...routes, { route_id: "overlap", method: "GET", path: "/items/{other}" }],
    [{ route_id: "wildcard", method: "GET", path: "/items/*" }],
    [],
  ]) {
    assert.throws(() => createWorkersHttpHandler({
      invoke() { return JSON.stringify({ routes: declared }); },
    }), TypeError);
  }
});
