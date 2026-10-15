import { expect, test } from "bun:test";
import { mkdtemp, realpath, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startApp } from "@lenso/core";
import { createLocalStoragePlugin } from "@lenso/storage/local";
import { migrateFiles } from "../src/files";
import { createRecipeApplication } from "../src/recipes/application";
import { inspectRecipes } from "../src/recipes/inspect";

const randomKey = () => Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString("hex");

test("borrowed Auth and DB power HTTP login and persistent owner-only files", async () => {
  const root = await mkdtemp(join(await realpath(tmpdir()), "lenso-recipe-test-"));
  const filename = join(root, "app.sqlite");
  const principals = [
    { subjectId: "alice", key: randomKey() },
    { subjectId: "bob", key: randomKey() },
  ];
  const assemble = () =>
    createRecipeApplication({
      filename,
      principals,
      storage: createLocalStoragePlugin({ id: "privateFiles", root: join(root, "objects") }),
    });
  try {
    await migrateFiles(filename);
    const recipe = assemble();
    expect(recipe.files!.requires).toContain(recipe.authentication);
    expect(recipe.files!.requires).toContain(recipe.database);
    expect(recipe.plugins.filter((plugin) => plugin === recipe.authentication)).toHaveLength(1);
    expect(recipe.plugins.filter((plugin) => plugin === recipe.database)).toHaveLength(1);
    let app = await startApp({ plugins: recipe.plugins });
    try {
      let web = app.get(recipe.web);
      const request = (path: string, init: RequestInit = {}) =>
        web.fetch(new Request(`http://local${path}`, init));
      const login = async (key: string) => {
        const response = await request("/session", {
          method: "POST",
          body: JSON.stringify({ key }),
        });
        expect(response.status).toBe(200);
        expect(response.headers.get("cache-control")).toBe("private, no-store");
        return (await response.json()) as { credential: string };
      };
      expect((await request("/session", { method: "POST", body: "{" })).status).toBe(400);
      expect((await request("/session", { method: "POST", body: "x".repeat(4097) })).status).toBe(
        400,
      );
      expect(
        (await request("/session", { method: "POST", body: JSON.stringify({ key: randomKey() }) }))
          .status,
      ).toBe(401);
      const alice = await login(principals[0]!.key),
        bob = await login(principals[1]!.key);
      const headers = { authorization: `Bearer ${alice.credential}` };
      const otherHeaders = { authorization: `Bearer ${bob.credential}` };
      expect(
        (
          await request("/files", {
            method: "PUT",
            body: "x",
            headers: { "content-length": "invalid" },
          })
        ).status,
      ).toBe(401);
      const uploaded = await request("/files", {
        method: "PUT",
        body: "private bytes",
        headers: {
          ...headers,
          "x-file-name": "private.txt",
          "content-type": "text/plain",
        },
      });
      expect(uploaded.status).toBe(201);
      const record = (await uploaded.json()) as {
        fileId: string;
        ownerId: string;
        storageId: string;
      };
      expect(record.ownerId).toBe("alice");
      expect(record.storageId).toBe(recipe.storage!.id);
      const path = `/files/${record.fileId}`;
      for (const method of ["GET", "HEAD", "DELETE"])
        expect((await request(path, { method, headers: otherHeaders })).status).toBe(403);
      const metadata = await request(path, { method: "HEAD", headers });
      expect(metadata.status).toBe(200);
      expect(metadata.headers.get("content-length")).toBe("13");
      expect(
        (await request("/files", { method: "PUT", headers, body: "x".repeat(1024 * 1024 + 1) }))
          .status,
      ).toBe(413);
      expect((await request("/session/renew", { method: "POST", headers })).status).toBe(409);
      await app.stop();
      const reopened = assemble();
      app = await startApp({ plugins: reopened.plugins });
      web = app.get(reopened.web);
      const read = await request(path, { headers });
      expect(read.status).toBe(200);
      expect(read.headers.get("cache-control")).toBe("private, no-store");
      expect(await read.text()).toBe("private bytes");
      expect((await request(path, { method: "DELETE", headers })).status).toBe(204);
      expect((await request("/session/revoke", { method: "POST", headers })).status).toBe(200);
      expect((await request("/files", { method: "PUT", headers, body: "revoked" })).status).toBe(
        401,
      );
    } finally {
      await app.stop();
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("login alone and files without login are explicit selectable HTTP entries", async () => {
  const root = await mkdtemp(join(await realpath(tmpdir()), "lenso-login-test-"));
  try {
    const filename = join(root, "login.sqlite");
    await migrateFiles(filename);
    const principals = [{ subjectId: "alice", key: randomKey() }];
    const recipe = createRecipeApplication({ filename, principals });
    expect(recipe.files).toBeUndefined();
    expect(recipe.plugins).toHaveLength(3);
    const app = await startApp({ plugins: recipe.plugins });
    try {
      expect((await app.get(recipe.web).fetch(new Request("http://local/files"))).status).toBe(404);
    } finally {
      await app.stop();
    }
    const onlyFiles = createRecipeApplication({
      filename,
      principals,
      login: false,
      storage: createLocalStoragePlugin({ id: "privateFiles", root: join(root, "objects") }),
    });
    const next = await startApp({ plugins: onlyFiles.plugins });
    try {
      expect(
        (
          await next
            .get(onlyFiles.web)
            .fetch(new Request("http://local/session", { method: "POST" }))
        ).status,
      ).toBe(404);
      const session = await next.get(onlyFiles.authentication).issue(principals[0]!.key);
      expect(
        (
          await next.get(onlyFiles.web).fetch(
            new Request("http://local/files", {
              method: "PUT",
              body: "host authenticated",
              headers: { authorization: `Bearer ${session.credential}` },
            }),
          )
        ).status,
      ).toBe(201);
    } finally {
      await next.stop();
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("static recipe diagnosis identifies provenance, omitted entries and exact missing dependencies without setup", async () => {
  const root = await mkdtemp(join(await realpath(tmpdir()), "lenso-inspect-recipes-"));
  try {
    const filename = join(root, "must-not-exist.sqlite");
    const source = `import { createRecipeApplication } from ${JSON.stringify(new URL("../src/recipes/application.ts", import.meta.url).pathname)};
const recipe = createRecipeApplication({ filename: ${JSON.stringify(filename)}, principals: () => { throw new Error("secret-do-not-read"); } });
`;
    await Bun.write(
      join(root, "login.ts"),
      source + "export default { plugins: recipe.plugins };\n",
    );
    const report = await inspectRecipes({ root, config: "login.ts" });
    expect("error" in report).toBe(false);
    if ("error" in report) throw new Error("Expected valid assembly");
    expect(report.capabilities).toEqual([
      { capability: "login", state: "declared" },
      { capability: "private-files", state: "entry-not-declared" },
    ]);
    expect(report.entries[0]).toMatchObject({
      adapter: "recipe-web",
      provider: "notes-auth",
      path: "/session",
    });
    expect(report.cliOperations).toEqual([]);
    expect(JSON.stringify(report)).not.toContain("secret-do-not-read");
    expect(
      await stat(filename).then(
        () => true,
        () => false,
      ),
    ).toBe(false);
    await Bun.write(
      join(root, "broken.ts"),
      source +
        "export default { plugins: recipe.plugins.filter(p => p !== recipe.authentication) };\n",
    );
    const broken = await inspectRecipes({ root, config: "broken.ts" });
    expect("error" in broken).toBe(true);
    expect(JSON.stringify(broken)).toContain("missing-dependency");
    expect(JSON.stringify(broken)).toContain("recipe-web");
    expect(JSON.stringify(broken)).toContain("notes-auth");
    expect(JSON.stringify(broken)).not.toContain("secret-do-not-read");
    expect(
      await stat(filename).then(
        () => true,
        () => false,
      ),
    ).toBe(false);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
