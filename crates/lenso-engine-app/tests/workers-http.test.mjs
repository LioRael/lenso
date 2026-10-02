import assert from "node:assert/strict";
import { test } from "node:test";
import { createWorkersHttpHandler } from "../assets/workers-http.mjs";

const routes = [
  { route_id: "item", method: "GET", path: "/items/{id}" },
  { route_id: "bytes", method: "POST", path: "/bytes" },
  { route_id: "reject", method: "GET", path: "/reject" },
  { route_id: "failure", method: "GET", path: "/failure" },
  { route_id: "failure-payload", method: "GET", path: "/failure-payload" },
  { route_id: "reject-other", method: "GET", path: "/reject-other" },
  { route_id: "custom", method: "FOO", path: "/custom" },
];

class ComponentError extends Error {
  constructor(payload) {
    super(payload);
    Object.defineProperty(this, "payload", { value: payload });
  }
}

function handler(calls = []) {
  return createWorkersHttpHandler({
    invoke(capability, operation, requestJson) {
      assert.equal(capability, "lenso.http.endpoint@1");
      if (operation === "describe") return JSON.stringify({ routes });
      const request = JSON.parse(requestJson);
      calls.push(request);
      if (request.route_id === "reject") throw new ComponentError('"rejected"');
      if (request.route_id === "reject-other") throw new ComponentError('"unauthorized"');
      if (request.route_id === "failure") throw new Error("guest failure");
      if (request.route_id === "failure-payload") {
        const error = new Error("runtime failure");
        error.payload = '"rejected"';
        throw error;
      }
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
  const customResponse = await fetch(new Request("http://127.0.0.1/custom", { method: "FoO" }));
  assert.equal(customResponse.status, 200);
  assert.equal(calls[2].method, "FOO");
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
  const methodNotAllowed = await fetch(new Request("http://127.0.0.1/items/42", { method: "POST" }));
  assert.equal(methodNotAllowed.status, 405);
  assert.equal(methodNotAllowed.headers.get("allow"), "GET");
  assert.equal((await fetch(new Request("http://127.0.0.1/reject"))).status, 502);
  assert.equal((await fetch(new Request("http://127.0.0.1/reject-other"))).status, 502);
  assert.equal((await fetch(new Request("http://127.0.0.1/failure"))).status, 503);
  assert.equal((await fetch(new Request("http://127.0.0.1/failure-payload"))).status, 503);
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

test("selects the requested method across overlapping paths and reports every allowed method", async () => {
  const calls = [];
  const fetch = createWorkersHttpHandler({
    invoke(_capability, operation, input) {
      if (operation === "describe") return JSON.stringify({ routes: [
        { route_id: "read", method: "GET", path: "/teams/{team}/items/{item}" },
        { route_id: "write", method: "POST", path: "/teams/{owner}/items/{id}" },
        { route_id: "literal", method: "DELETE", path: "/teams/a/items/b" },
        { route_id: "root", method: "GET", path: "/" },
      ] });
      calls.push(JSON.parse(input));
      return JSON.stringify({ status: 200, headers: [], body: "" });
    },
  });
  for (const [method, routeId, names] of [
    ["GET", "read", ["team", "item"]],
    ["POST", "write", ["owner", "id"]],
    ["DELETE", "literal", []],
  ]) {
    const response = await fetch(new Request("http://fixture.invalid/teams/a/items/b", { method }));
    assert.equal(response.status, 200);
    const selected = calls.at(-1);
    assert.equal(selected.route_id, routeId);
    assert.deepEqual(selected.path_parameters.map(({ name }) => name), names);
    assert.deepEqual(selected.path_parameters.map(({ value }) => value), names.length ? ["a", "b"] : []);
  }
  for (const [path, allow] of [
    ["/teams/a/items/b", "DELETE, GET, POST"],
    ["/teams/c/items/d", "GET, POST"],
    ["/", "GET"],
  ]) {
    const response = await fetch(new Request(`http://fixture.invalid${path}`, { method: "PUT" }));
    assert.equal(response.status, 405);
    assert.equal(response.headers.get("allow"), allow);
  }
  assert.equal((await fetch(new Request("http://fixture.invalid/"))).status, 200);
  assert.deepEqual(calls.at(-1).path_parameters, []);
  const invoked = calls.length;
  for (const path of ["/teams/a/items/b/", "/teams//items/b", "/absent"]) {
    assert.equal((await fetch(new Request(`http://fixture.invalid${path}`))).status, 404);
  }
  assert.equal(calls.length, invoked);
});

test("routes a maximum-size manifest and preserves late-match parameters", async () => {
  const calls = [];
  const fetch = createWorkersHttpHandler({
    invoke(_capability, operation, input) {
      if (operation === "describe") return JSON.stringify({ routes: Array.from({ length: 256 }, (_, index) =>
        ({ route_id: `r${index}`, method: "GET", path: `/r${index}/{id}` })) });
      calls.push(JSON.parse(input));
      return JSON.stringify({ status: 200, headers: [], body: "" });
    },
  });
  for (const index of [0, 128, 255]) {
    assert.equal((await fetch(new Request(`http://fixture.invalid/r${index}/value?mode=raw`))).status, 200);
    assert.equal(calls.at(-1).route_id, `r${index}`);
    assert.deepEqual(calls.at(-1).path_parameters, [{ name: "id", value: "value" }]);
    assert.equal(calls.at(-1).query, "mode=raw");
  }
  const rejected = await fetch(new Request("http://fixture.invalid/r255/value", { method: "POST" }));
  assert.equal(rejected.status, 405);
  assert.equal(rejected.headers.get("allow"), "GET");
  assert.equal(calls.length, 3);
});

test("preserves every byte at the body limit and rejects noncanonical or oversized response encodings", async () => {
  const bytes = Uint8Array.from({ length: 65_536 }, (_, index) => index % 256);
  const response = await handler()(new Request("http://fixture.invalid/bytes", { method: "POST", body: bytes }));
  assert.equal(response.status, 200);
  assert.deepEqual(new Uint8Array(await response.arrayBuffer()), bytes);
  for (const body of ["AA=", "AB==", "AA==\n", btoa("x".repeat(65_537))]) {
    const invalid = createWorkersHttpHandler({
      invoke(_capability, operation) {
        return operation === "describe" ? JSON.stringify({ routes }) : JSON.stringify({ status: 200, headers: [], body });
      },
    });
    const rejected = await invalid(new Request("http://fixture.invalid/items/42"));
    assert.equal(rejected.status, 502);
    assert.deepEqual(await rejected.json(), { error: "invalid_endpoint_response" });
  }
});
