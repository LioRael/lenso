const CAPABILITY = "lenso.http.endpoint@1";
const MAX_BODY = 65_536;
const MAX_HEADER_BYTES = 32_768;
const MAX_HEADER_COUNT = 128;
const MAX_ROUTES = 256;
const encoder = new TextEncoder();
const OWNED_REQUEST_HEADERS = new Set([
  "authorization", "cookie", "connection", "te", "trailer", "transfer-encoding",
  "upgrade", "keep-alive", "proxy-connection", "content-length", "host",
  "x-request-id",
]);
const OWNED_RESPONSE_HEADERS = new Set([
  "connection", "te", "trailer", "transfer-encoding", "upgrade", "keep-alive",
  "proxy-connection", "content-length", "set-cookie", "x-content-type-options", "x-request-id",
]);

function failure(status, code, requestId) {
  const headers = new Headers({ "cache-control": "no-store", "x-content-type-options": "nosniff" });
  if (requestId) headers.set("x-request-id", requestId);
  return Response.json({ error: code }, { status, headers });
}

function routeSegments(path) {
  if (typeof path !== "string" || !path.startsWith("/") || path.includes("?") ||
      path.includes("#") || path.includes("%") || path.includes("//")) {
    throw new TypeError("Workers HTTP Endpoint has an unsupported route path");
  }
  if (path === "/") return [];
  const segments = path.slice(1).split("/");
  if (segments.some((segment) => !segment))
    throw new TypeError("Workers HTTP Endpoint has an empty route segment");
  return segments.map((segment) => {
    const parameter = /^\{([A-Za-z_][A-Za-z0-9_]*)\}$/.exec(segment);
    if (parameter) return { parameter: parameter[1] };
    if (!/^[A-Za-z0-9._~-]+$/.test(segment))
      throw new TypeError("Workers HTTP Endpoint route exceeds the local-workerd path profile");
    return { literal: segment };
  });
}

function routesOverlap(left, right) {
  return left.length === right.length && left.every((segment, index) =>
    segment.parameter || right[index].parameter || segment.literal === right[index].literal);
}

function selectedRoutes(component) {
  const description = JSON.parse(component.invoke(CAPABILITY, "describe", "{}"));
  if (!description || !Array.isArray(description.routes) ||
      description.routes.length === 0 || description.routes.length > MAX_ROUTES) {
    throw new TypeError("Workers HTTP Endpoint needs a bounded route description");
  }
  const routes = description.routes.map((route) => {
    if (!route || typeof route.route_id !== "string" || !route.route_id.trim() ||
        typeof route.method !== "string" || !/^[A-Z]+$/.test(route.method)) {
      throw new TypeError("Workers HTTP Endpoint declared an invalid route");
    }
    return { routeId: route.route_id, method: route.method, segments: routeSegments(route.path) };
  });
  for (let index = 0; index < routes.length; index++) {
    for (let other = 0; other < index; other++) {
      if (routes[index].method === routes[other].method &&
          routesOverlap(routes[index].segments, routes[other].segments)) {
        throw new TypeError("Workers HTTP Endpoint has overlapping routes");
      }
    }
  }
  return routes;
}

function matchPath(route, path) {
  const actual = path === "/" ? [] : path.slice(1).split("/");
  if (actual.length !== route.segments.length) return null;
  const parameters = [];
  for (let index = 0; index < actual.length; index++) {
    const expected = route.segments[index];
    if (expected.literal !== undefined) {
      if (actual[index] !== expected.literal) return null;
    } else {
      if (!actual[index]) return null;
      parameters.push({ name: expected.parameter, value: actual[index] });
    }
  }
  return parameters;
}

function credentialEvidence(headers) {
  const authorization = headers.get("authorization");
  if (authorization === null) return null;
  const separator = authorization.indexOf(" ");
  if (separator <= 0 || separator === authorization.length - 1) throw new TypeError("bad authorization");
  const scheme = authorization.slice(0, separator).toLowerCase();
  const value = authorization.slice(separator + 1);
  if ((scheme === "bearer" || scheme === "basic") && /[,\s]/.test(value))
    throw new TypeError("ambiguous authorization");
  return { scheme, value };
}

function forwardedHeaders(headers) {
  const forwarded = [];
  let bytes = 0;
  let count = 0;
  const connectionOwned = new Set((headers.get("connection") ?? "").split(",")
    .map((name) => name.trim().toLowerCase()).filter(Boolean));
  for (const [name, value] of headers) {
    bytes += encoder.encode(name).byteLength + encoder.encode(value).byteLength;
    count++;
    if (bytes > MAX_HEADER_BYTES || count > MAX_HEADER_COUNT)
      throw new TypeError("request headers too large");
    if (!OWNED_REQUEST_HEADERS.has(name) && !connectionOwned.has(name))
      forwarded.push({ name, value });
  }
  return forwarded;
}

async function readBoundedBody(request) {
  if (!request.body) return new Uint8Array();
  const reader = request.body.getReader();
  const chunks = [];
  let length = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > MAX_BODY) {
        await reader.cancel();
        return null;
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const result = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return result;
}

function encodeBody(bytes) {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function decodeBody(value) {
  if (typeof value !== "string" || value.length > Math.ceil(MAX_BODY / 3) * 4 ||
      !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)) {
    throw new TypeError("invalid Endpoint body");
  }
  const binary = atob(value);
  if (binary.length > MAX_BODY || btoa(binary) !== value) throw new TypeError("noncanonical Endpoint body");
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function endpointResponse(result, requestId) {
  if (!result || !Number.isInteger(result.status) || result.status < 200 || result.status > 599 ||
      !Array.isArray(result.headers)) throw new TypeError("invalid Endpoint response");
  const body = decodeBody(result.body);
  if ([204, 205, 304].includes(result.status) && body.length)
    throw new TypeError("Endpoint body forbidden for response status");
  const headers = new Headers();
  let headerBytes = 0;
  let headerCount = 0;
  for (const header of result.headers) {
    if (!header || typeof header.name !== "string" || typeof header.value !== "string" ||
        OWNED_RESPONSE_HEADERS.has(header.name.toLowerCase())) {
      throw new TypeError("Endpoint returned an ingress-owned response header");
    }
    headerBytes += encoder.encode(header.name).byteLength + encoder.encode(header.value).byteLength;
    headerCount++;
    if (headerBytes > MAX_HEADER_BYTES || headerCount > MAX_HEADER_COUNT)
      throw new TypeError("Endpoint response headers exceed the Workers profile");
    headers.append(header.name, header.value);
  }
  headers.set("x-content-type-options", "nosniff");
  headers.set("x-request-id", requestId);
  return new Response([204, 205, 304].includes(result.status) ? null : body,
    { status: result.status, headers });
}

export function createWorkersHttpHandler(component) {
  const routes = selectedRoutes(component);
  return async function fetch(request) {
    const requestId = crypto.randomUUID();
    const url = new URL(request.url);
    if (url.pathname.includes("%")) return failure(400, "unsupported_path_encoding", requestId);
    if (request.method === "CONNECT" || request.headers.has("upgrade") || request.headers.has("cookie"))
      return failure(400, "unsupported_transport", requestId);
    const pathMatches = routes.map((route) => ({ route, parameters: matchPath(route, url.pathname) }))
      .filter((match) => match.parameters !== null);
    if (!pathMatches.length) return failure(404, "not_found", requestId);
    const match = pathMatches.find(({ route }) => route.method === request.method);
    if (!match) return failure(405, "method_not_allowed", requestId);
    let credential;
    let headers;
    try {
      credential = credentialEvidence(request.headers);
      headers = forwardedHeaders(request.headers);
    } catch {
      return failure(400, "bad_request", requestId);
    }
    if (Number(request.headers.get("content-length")) > MAX_BODY)
      return failure(413, "request_too_large", requestId);
    let body;
    try {
      body = await readBoundedBody(request);
    } catch {
      return failure(400, "bad_request", requestId);
    }
    if (body === null) return failure(413, "request_too_large", requestId);
    const payload = {
      route_id: match.route.routeId,
      request_id: requestId,
      method: request.method,
      path: url.pathname,
      path_parameters: match.parameters,
      headers,
      body: encodeBody(body),
      ...(credential ? { credential } : {}),
      ...(url.search ? { query: url.search.slice(1) } : {}),
    };
    let result;
    try {
      result = JSON.parse(component.invoke(CAPABILITY, "handle", JSON.stringify(payload)));
    } catch (error) {
      return error?.payload === '"rejected"'
        ? failure(502, "endpoint_rejected", requestId)
        : failure(503, "endpoint_unavailable", requestId);
    }
    try {
      return endpointResponse(result, requestId);
    } catch {
      return failure(502, "invalid_endpoint_response", requestId);
    }
  };
}
