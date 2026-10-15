import { AuthError } from "@lenso/auth";
import { bearerEvidence } from "@lenso/auth/fetch";
import type { WebContext } from "@lenso/web";
import type { NotesAuthentication } from "../auth";

export const sessionPaths = {
  login: "/session",
  renew: "/session/renew",
  revoke: "/session/revoke",
} as const;

export class RecipeInputError extends Error {}

async function loginKey(request: Request): Promise<string> {
  // Bound the actual stream, including requests without Content-Length.
  const reader = request.body?.getReader();
  if (!reader) throw new RecipeInputError("JSON login body required");
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > 4096) throw new RecipeInputError("Login body exceeds 4096 bytes");
      chunks.push(value);
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    const data = JSON.parse(new TextDecoder().decode(bytes));
    if (
      typeof data !== "object" ||
      data === null ||
      Object.keys(data).length !== 1 ||
      typeof data.key !== "string" ||
      !/^[0-9a-fA-F]{64}$/.test(data.key)
    )
      throw new RecipeInputError("Expected a 32-byte hex login key");
    return data.key;
  } catch (error) {
    request.signal.throwIfAborted();
    if (error instanceof RecipeInputError) throw error;
    throw new RecipeInputError("Invalid login JSON");
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

/** Uses the host's existing Notes Auth; no verifier, session store or cookie is created here. */
export function createLoginRoutes(auth: NotesAuthentication) {
  return async ({ request, signal }: WebContext): Promise<Response | undefined> => {
    const path = new URL(request.url).pathname;
    if (!Object.values(sessionPaths).some((candidate) => candidate === path)) return undefined;
    if (request.method !== "POST")
      return new Response(null, { status: 405, headers: { allow: "POST" } });
    const headers = { "cache-control": "private, no-store" };
    if (path === sessionPaths.login)
      return Response.json(await auth.issue(await loginKey(request), { signal }), { headers });
    const evidence = bearerEvidence({ request, signal });
    if (!evidence.evidence) throw new AuthError("UNAUTHORIZED");
    if (path === sessionPaths.renew)
      return Response.json(await auth.renew(evidence.evidence, evidence), { headers });
    await auth.revoke(evidence.evidence, evidence);
    return Response.json({ revoked: true }, { headers });
  };
}
