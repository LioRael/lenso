import { mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { defineSource } from "@lenso/auth";
import { withApp } from "@lenso/core";
import { createNotesApp } from "./app";

// Local demonstration only: identity comparison with an unexported random proof.
// Replace this verifier with a real trusted Auth source before exposing an entry.
const proof = crypto.randomUUID();
const source = defineSource({
  async verify(evidence: string) {
    return evidence === proof
      ? { status: "verified" as const, subjectId: "alice" }
      : { status: "rejected" as const };
  },
});
const root = await mkdtemp(join(await realpath(tmpdir()), "lenso-assembly-"));
try {
  const { app, notes } = createNotesApp(root, source);
  const text = await withApp(app, async (running) => {
    const service = running.get(notes);
    await service.put(proof, "alice", "Hello from Auth + Storage + Cache");
    return service.read(proof, "alice");
  });
  console.log(text);
} finally {
  await rm(root, { recursive: true, force: true });
}
