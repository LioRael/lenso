import { lookup } from "node:dns/promises";
import type { ClientRequest } from "node:http";
import { request } from "node:https";
import { isIP } from "node:net";
import { checkServerIdentity, type TLSSocket } from "node:tls";

export interface OutboundPolicy {
  readonly allowedHosts: readonly string[];
  readonly dnsTimeoutMs: number;
  readonly connectTimeoutMs: number;
  readonly timeoutMs: number;
  readonly maxRequestBytes: number;
  readonly maxResponseBytes: number;
}

export interface HttpResult {
  readonly status: number;
  readonly retryAfter: string | null;
}

type ErrorCode =
  | "policy-rejected"
  | "timeout"
  | "connection-failed"
  | "response-too-large"
  | "request-too-large"
  | "redirect-rejected";

export class OutboundError extends Error {
  readonly code: ErrorCode;
  constructor(code: ErrorCode) {
    super(`Webhook outbound ${code}`);
    this.name = "OutboundError";
    this.code = code;
  }
}

const hostnamePattern =
  /^(?=.{1,253}$)(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z](?:[a-z0-9-]{0,61}[a-z0-9])?$/;

function validatePolicy(policy: OutboundPolicy): void {
  if (
    !policy.allowedHosts.length ||
    policy.allowedHosts.some((host) => !hostnamePattern.test(host) || isIP(host) !== 0) ||
    [
      policy.dnsTimeoutMs,
      policy.connectTimeoutMs,
      policy.timeoutMs,
      policy.maxRequestBytes,
      policy.maxResponseBytes,
    ].some((value) => !Number.isSafeInteger(value) || value <= 0 || value > 2_147_483_647)
  ) {
    throw new OutboundError("policy-rejected");
  }
}

export function validateEndpointUrl(value: string, policy: OutboundPolicy): string {
  validatePolicy(policy);
  try {
    if (
      value.length > 8192 ||
      /[\s\\%]/.test(value.split(/[/?#]/).slice(0, 3).join("/")) ||
      [...value].some(
        (character) => character.charCodeAt(0) <= 32 || character.charCodeAt(0) === 127,
      ) ||
      value.includes("#")
    ) {
      throw new Error();
    }
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      (url.port !== "" && url.port !== "443") ||
      url.username ||
      url.password ||
      !hostnamePattern.test(url.hostname) ||
      isIP(url.hostname) !== 0 ||
      !policy.allowedHosts.includes(url.hostname)
    )
      throw new Error();
    return url.href;
  } catch {
    throw new OutboundError("policy-rejected");
  }
}

function publicAddress(address: string): boolean {
  if (isIP(address) === 4) {
    const [a, b, c] = address.split(".").map(Number) as [number, number, number, number];
    return !(
      a === 0 ||
      a === 10 ||
      a === 127 ||
      a >= 224 ||
      address === "168.63.129.16" ||
      (a === 100 && b >= 64 && b <= 127) ||
      (a === 169 && b === 254) ||
      (a === 172 && b >= 16 && b <= 31) ||
      (a === 192 && (b === 168 || b === 0 || (b === 88 && c === 99) || b === 2)) ||
      (a === 198 && (b === 18 || b === 19 || (b === 51 && c === 100))) ||
      (a === 203 && b === 0 && c === 113)
    );
  }
  if (isIP(address) !== 6 || address.includes(".")) return false;
  const canonical = new URL(`http://[${address}]/`).hostname.slice(1, -1).toLowerCase();
  const first = Number.parseInt(canonical.split(":")[0]!, 16);
  const second = Number.parseInt(canonical.split(":")[1] || "0", 16);
  // Only ordinary global unicast; exclude protocol assignments, documentation and 6to4.
  return (
    first >= 0x2000 &&
    first <= 0x3fff &&
    !(first === 0x2001 && (second < 0x200 || second === 0xdb8)) &&
    !canonical.startsWith("2002:") &&
    !canonical.startsWith("3fff:")
  );
}

interface Address {
  address: string;
  family: number;
}
interface TestDependencies {
  readonly resolve: (host: string) => Promise<readonly Address[]>;
  readonly isPublicAddress?: (address: string) => boolean;
  readonly ca: string;
  readonly loopbackTlsPort?: number;
}

/** Internal test seam. Never re-export from the package entry point. */
export function createPinnedHttpsTransportForTest(
  policy: OutboundPolicy,
  dependencies: TestDependencies,
) {
  if (
    dependencies.loopbackTlsPort !== undefined &&
    (!Number.isInteger(dependencies.loopbackTlsPort) ||
      dependencies.loopbackTlsPort < 1024 ||
      dependencies.loopbackTlsPort > 65535)
  )
    throw new OutboundError("policy-rejected");
  return createTransport(policy, {
    ...dependencies,
    isPublicAddress: dependencies.isPublicAddress ?? publicAddress,
  });
}

export function createPinnedHttpsTransport(policy: OutboundPolicy) {
  return createTransport(policy, {
    resolve: (host) => lookup(host, { all: true, verbatim: true }),
    isPublicAddress: publicAddress,
  });
}

function createTransport(
  policy: OutboundPolicy,
  dependencies: {
    resolve: (host: string) => Promise<readonly Address[]>;
    isPublicAddress: (address: string) => boolean;
    ca?: string;
    loopbackTlsPort?: number;
  },
) {
  validatePolicy(policy);
  const frozenPolicy = { ...policy, allowedHosts: [...policy.allowedHosts] };
  return {
    async send(input: {
      url: string;
      body: Uint8Array;
      headers: Readonly<Record<string, string>>;
      signal: AbortSignal;
    }): Promise<HttpResult> {
      const url = new URL(validateEndpointUrl(input.url, frozenPolicy));
      if (input.body.byteLength > frozenPolicy.maxRequestBytes)
        throw new OutboundError("request-too-large");
      return new Promise<HttpResult>((resolve, reject) => {
        let req: ClientRequest | undefined;
        let settled = false;
        let connectTimer: ReturnType<typeof setTimeout> | undefined;
        const cleanup = () => {
          clearTimeout(totalTimer);
          clearTimeout(dnsTimer);
          clearTimeout(connectTimer);
          input.signal.removeEventListener("abort", abort);
        };
        const fail = (code: ErrorCode) => {
          if (settled) return;
          settled = true;
          cleanup();
          req?.destroy();
          reject(new OutboundError(code));
        };
        const abort = () => fail("timeout");
        const totalTimer = setTimeout(() => fail("timeout"), frozenPolicy.timeoutMs);
        const dnsTimer = setTimeout(() => fail("timeout"), frozenPolicy.dnsTimeoutMs);
        input.signal.addEventListener("abort", abort, { once: true });
        if (input.signal.aborted) {
          fail("timeout");
          return;
        }
        Promise.resolve()
          .then(() => dependencies.resolve(url.hostname))
          .then((addresses) => {
            if (settled) return;
            clearTimeout(dnsTimer);
            if (
              !addresses.length ||
              addresses.some(
                (answer) =>
                  isIP(answer.address) !== answer.family ||
                  !dependencies.isPublicAddress(answer.address) ||
                  (dependencies.loopbackTlsPort !== undefined &&
                    !(answer.family === 4 && answer.address.startsWith("127."))),
              )
            ) {
              fail("policy-rejected");
              return;
            }
            const chosen = addresses[0]!;
            const headers: Record<string, string> = {};
            for (const [name, value] of Object.entries(input.headers)) {
              if (
                [
                  "host",
                  "content-length",
                  "transfer-encoding",
                  "connection",
                  "upgrade",
                  "expect",
                  "proxy-authorization",
                  "proxy-connection",
                ].includes(name.toLowerCase())
              ) {
                fail("policy-rejected");
                return;
              }
              headers[name] = value;
            }
            connectTimer = setTimeout(() => fail("timeout"), frozenPolicy.connectTimeoutMs);
            try {
              let identityChecked = false;
              // Numeric hostname bypasses all subsequent DNS, including a rebinding answer.
              req = request(
                {
                  hostname: chosen.address,
                  family: chosen.family,
                  port: dependencies.loopbackTlsPort ?? 443,
                  servername: url.hostname,
                  method: "POST",
                  path: url.pathname + url.search,
                  maxHeaderSize: 16_384,
                  agent: false,
                  rejectUnauthorized: true,
                  ca: dependencies.ca,
                  checkServerIdentity: (_host, cert) => {
                    const error = checkServerIdentity(url.hostname, cert);
                    identityChecked = error === undefined;
                    return error;
                  },
                  headers: {
                    ...headers,
                    host: url.hostname,
                    "content-length": String(input.body.byteLength),
                    connection: "close",
                  },
                },
                (response) => {
                  if (settled) {
                    response.destroy();
                    return;
                  }
                  const status = response.statusCode ?? 0;
                  if (status >= 300 && status < 400) {
                    fail("redirect-rejected");
                    response.destroy();
                    return;
                  }
                  let bytes = 0;
                  response.on("data", (chunk: Buffer) => {
                    bytes += chunk.byteLength;
                    if (bytes > frozenPolicy.maxResponseBytes) {
                      fail("response-too-large");
                      response.destroy();
                    }
                  });
                  response.on("error", () => fail("connection-failed"));
                  response.on("aborted", () => fail("connection-failed"));
                  response.on("end", () => {
                    if (settled) return;
                    settled = true;
                    cleanup();
                    const retryAfter = response.headers["retry-after"];
                    resolve({
                      status,
                      retryAfter:
                        typeof retryAfter === "string" && retryAfter.length <= 128
                          ? retryAfter
                          : null,
                    });
                  });
                },
              );
              req.on("socket", (socket) =>
                socket.once("secureConnect", () => {
                  if (settled) return;
                  const tls = socket as TLSSocket;
                  const normalize = (address: string) =>
                    isIP(address) === 6 ? new URL(`http://[${address}]/`).hostname : address;
                  // Do not release the body on a runtime that silently ignores TLS options.
                  if (
                    !tls.encrypted ||
                    !tls.authorized ||
                    !identityChecked ||
                    !tls.remoteAddress ||
                    normalize(tls.remoteAddress) !== normalize(chosen.address)
                  ) {
                    fail("connection-failed");
                    return;
                  }
                  clearTimeout(connectTimer);
                  req!.end(input.body);
                }),
              );
              req.on("error", () => fail("connection-failed"));
            } catch {
              fail("connection-failed");
            }
          })
          .catch(() => fail("connection-failed"));
      });
    },
  };
}
