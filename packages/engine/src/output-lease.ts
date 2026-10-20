import { lstat, open, readFile, unlink } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { EngineError } from "./diagnostics";
import type { EngineMode } from "./engine-authoring";

/** Exclusive application output ownership, not a queue or a stale-lock reaper. */
export async function acquireOutputLease(
  path: string,
  mode: EngineMode,
): Promise<() => Promise<void>> {
  const fail = (code: string, message: string, cause?: unknown) =>
    new EngineError({ code, phase: "engine-output", message, source: { file: path } }, { cause });
  let file;
  try {
    // wx rejects existing regular files, directories and symlinks atomically.
    file = await open(path, "wx", 0o600);
  } catch (cause) {
    if ((cause as NodeJS.ErrnoException).code === "EEXIST")
      throw fail(
        "engine-output-in-use",
        "Application output is already claimed. Use a separate worktree/application root. Stop the owning session first; an abandoned claim requires verified manual recovery and is never automatically removed.",
        cause,
      );
    throw fail("engine-output-lock-failed", "Cannot claim application output.", cause);
  }
  const identity = await file.stat();
  const content = `${JSON.stringify({ schemaVersion: 1, pid: process.pid, mode, token: randomUUID() })}\n`;
  try {
    await file.writeFile(content);
  } catch (cause) {
    // Preserve an incomplete claim: ownership cannot be proven from its content.
    await file.close();
    throw fail("engine-output-lock-failed", "Cannot record application output ownership.", cause);
  }
  return async () => {
    try {
      const current = await lstat(path);
      if (
        !current.isFile() ||
        current.dev !== identity.dev ||
        current.ino !== identity.ino ||
        (await readFile(path, "utf8")) !== content
      )
        throw fail("engine-output-lock-changed", "Output claim changed; it was not removed.");
      await unlink(path);
    } catch (cause) {
      if (cause instanceof EngineError) throw cause;
      throw fail(
        "engine-output-lock-changed",
        "Cannot verify and release output ownership.",
        cause,
      );
    } finally {
      await file.close();
    }
  };
}
