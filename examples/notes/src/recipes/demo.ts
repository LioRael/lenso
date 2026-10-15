import { mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startApp } from "@lenso/core";
import { createLocalStoragePlugin } from "@lenso/storage/local";
import { createBunListenerPlugin } from "@lenso/web/bun";
import { migrateFiles } from "../files";
import { createRecipeApplication } from "./application";

/** Disposable local end-to-end fixture, using synthetic credentials which are never logged. */
export async function runRecipeDemo() {
  const started = performance.now();
  const root = await mkdtemp(join(await realpath(tmpdir()), "lenso-recipes-"));
  try {
    const filename = join(root, "app.sqlite");
    const randomKey = () => Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString("hex");
    const alice = randomKey(),
      bob = randomKey();
    // An explicit fixture migration, never performed by an imported config or plugin setup.
    await migrateFiles(filename);
    const recipe = createRecipeApplication({
      filename,
      principals: [
        { subjectId: "alice", key: alice },
        { subjectId: "bob", key: bob },
      ],
      storage: createLocalStoragePlugin({ id: "privateFiles", root: join(root, "objects") }),
    });
    const listener = createBunListenerPlugin({
      web: recipe.web,
      hostname: "127.0.0.1",
      port: 0,
      ingress: () => undefined,
    });
    const app = await startApp({ plugins: [...recipe.plugins, listener] });
    try {
      const url = app.get(listener).url;
      const request = async (path: string, init: RequestInit, expected: number) => {
        const response = await fetch(new URL(path, url), init);
        if (response.status !== expected) {
          await response.body?.cancel();
          throw new Error(
            `Recipe check ${init.method ?? "GET"} ${path} expected ${expected}, received ${response.status}`,
          );
        }
        return response;
      };
      const login = async (key: string) =>
        (await (
          await request(
            "/session",
            {
              method: "POST",
              body: JSON.stringify({ key }),
              headers: { "content-type": "application/json" },
            },
            200,
          )
        ).json()) as { credential: string };
      const a = await login(alice),
        b = await login(bob);
      const headers = { authorization: `Bearer ${a.credential}` };
      const content = "Private attachment from the same Auth and database.\n";
      const uploaded = (await (
        await request(
          "/files",
          {
            method: "PUT",
            body: content,
            headers: { ...headers, "content-type": "text/plain", "x-file-name": "private.txt" },
          },
          201,
        )
      ).json()) as { fileId: string };
      const path = `/files/${uploaded.fileId}`;
      const read = await request(path, { headers }, 200);
      if ((await read.text()) !== content) throw new Error("Private upload round trip failed");
      await (
        await request(path, { headers: { authorization: `Bearer ${b.credential}` } }, 403)
      ).body?.cancel();
      await (await request("/session/renew", { method: "POST", headers }, 409)).body?.cancel();
      await (await request(path, { method: "DELETE", headers }, 204)).body?.cancel();
      await (await request("/session/revoke", { method: "POST", headers }, 200)).body?.cancel();
      await (
        await request("/files", { method: "PUT", headers, body: "revoked" }, 401)
      ).body?.cancel();
      return {
        login: "passed",
        privateUploadAndRead: "passed",
        otherUser: 403,
        renewalBeforeDue: 409,
        revokedSession: 401,
        transport: "loopback HTTP",
        elapsedMs: Math.round(performance.now() - started),
      };
    } finally {
      await app.stop();
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

if (import.meta.main) console.log(JSON.stringify(await runRecipeDemo(), null, 2));
