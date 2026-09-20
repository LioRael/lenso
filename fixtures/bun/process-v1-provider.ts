import { serveProcessV1 } from "@lenso/bun";
import { bindProvider, type Provider } from "./generated/greeting.ts";

const greeting: Provider = {
  async greet(_context, request) {
    if (request.name === "Slow") await Bun.sleep(100);
    return { ok: true, value: { message: `Hello from Process V1, ${request.name}!` } };
  },
};

const generated = bindProvider(greeting);
serveProcessV1([{
  ...generated,
  descriptor: {
    ...generated.descriptor,
    descriptor_digest: `sha256:${"d".repeat(64)}`,
  },
  validateRequest(operation, payload) {
    if (
      operation !== "greet" || typeof payload !== "object" || payload === null ||
      typeof (payload as { name?: unknown }).name !== "string"
    ) throw new Error("invalid greet request");
  },
  validateSuccess(operation, payload) {
    if (
      operation !== "greet" || typeof payload !== "object" || payload === null ||
      typeof (payload as { message?: unknown }).message !== "string"
    ) throw new Error("invalid greet response");
  },
  validateDomainError(operation) {
    if (operation !== "greet") throw new Error("invalid greet Domain Error");
  },
}], Object.freeze({}));
