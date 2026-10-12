import type { LoginMethod } from "./browser";
import { AuthError } from "./errors";
import { interactiveOwners } from "./interactive-owner";
import type { ManagedSession } from "./sessions";
import type { AuthSource } from "./source";

export interface PasswordInput {
  readonly identifier: string;
  readonly password: string;
}

export interface InteractiveContext {
  readonly signal: AbortSignal;
  /** Already checked against the binding's configured local return roots. */
  readonly returnPath: string;
}

export interface InteractiveLogin {
  readonly descriptor: LoginMethod;
  readonly method: "GET" | "POST";
  readonly callback?: {
    readonly path: string;
    readonly method: "GET" | "POST";
    complete(request: Request, context: InteractiveContext): Promise<Response>;
  };
  start(request: Request, context: InteractiveContext): Promise<ManagedSession | Response>;
}

/** Verification sources are not login capabilities. Install this explicitly. */
export function passwordLogin(options: {
  readonly id: string;
  readonly label: string;
  readonly path: string;
  readonly sessions: {
    readonly source: AuthSource<string | null, string>;
    issue(input: PasswordInput, options: { signal: AbortSignal }): Promise<ManagedSession>;
    revoke(credential: string, options: { signal: AbortSignal }): Promise<void>;
  };
}): InteractiveLogin {
  const login: InteractiveLogin = Object.freeze({
    descriptor: Object.freeze({
      id: options.id,
      kind: "password" as const,
      label: options.label,
      action: options.path,
    }),
    method: "POST" as const,
    async start(request: Request, context: InteractiveContext) {
      if (request.headers.get("content-type")?.split(";")[0]?.trim() !== "application/json")
        return new Response(null, { status: 415 });
      const reader = request.body?.getReader();
      if (!reader) return new Response(null, { status: 400 });
      const chunks: Uint8Array[] = [];
      let length = 0;
      try {
        while (true) {
          context.signal.throwIfAborted();
          const part = await reader.read();
          if (part.done) break;
          length += part.value.byteLength;
          if (length > 8192) {
            await reader.cancel();
            return new Response(null, { status: 413 });
          }
          chunks.push(part.value);
        }
      } finally {
        reader.releaseLock();
      }
      const bytes = new Uint8Array(length);
      let offset = 0;
      for (const chunk of chunks) {
        bytes.set(chunk, offset);
        offset += chunk.length;
      }
      let input: unknown;
      try {
        input = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
      } catch {
        return new Response(null, { status: 400 });
      }
      if (
        !input ||
        typeof input !== "object" ||
        !("identifier" in input) ||
        typeof input.identifier !== "string" ||
        !input.identifier.trim() ||
        input.identifier.length > 512 ||
        !("password" in input) ||
        typeof input.password !== "string" ||
        !input.password ||
        input.password.length > 4096
      )
        throw new AuthError("UNAUTHORIZED");
      context.signal.throwIfAborted();
      const issued = await options.sessions.issue(
        { identifier: input.identifier, password: input.password },
        { signal: context.signal },
      );
      const verified = await options.sessions.source.verify(issued.credential, {
        signal: context.signal,
      });
      if (verified.status !== "verified" || verified.kind === "service") {
        await options.sessions.revoke(issued.credential, { signal: context.signal });
        throw new AuthError("UNAUTHORIZED");
      }
      return issued;
    },
  });
  interactiveOwners.set(login, options.sessions);
  return login;
}

/** The host adapter owns its protocol, state/nonce, callback validation and external cookies.
 * This extension does not implement OIDC, OAuth, SAML, or turn sessionSource into login.
 */
export function externalEntry(options: {
  readonly descriptor: Exclude<LoginMethod, { kind: "password" }>;
  readonly callback?: InteractiveLogin["callback"];
  start(request: Request, context: InteractiveContext): Promise<Response>;
}): InteractiveLogin {
  return Object.freeze({
    descriptor: options.descriptor,
    method: options.descriptor.kind === "redirect" ? "GET" : "POST",
    ...(options.callback ? { callback: Object.freeze({ ...options.callback }) } : {}),
    start: options.start,
  });
}
