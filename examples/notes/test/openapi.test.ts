import { expect, test } from "bun:test";
import { mkdtemp, realpath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startApp } from "@lenso/core";
import { createORPCClient } from "@orpc/client";
import { OpenAPILink } from "@orpc/openapi/fetch";
import { RPCLink } from "@orpc/client/fetch";
import { RPCHandler } from "@orpc/server/fetch";
import type { RouterClient } from "@orpc/server";
import { createProblemDetailsDecoder } from "@lenso/web/openapi-client";
import { createNotesFiles, migrateFiles } from "../src/files";
import { createNotesRouter } from "../src/router";
import { createNotesOpenAPI } from "../src/openapi";
import { notesAudiences } from "../src/notes";
import { createNotesWebPlugin } from "../src/web";

test("optional Notes OpenAPI shares authorized RPC services and publishes only selected schemas", async () => {
  const directory = await mkdtemp(join(await realpath(tmpdir()), "notes-openapi-"));
  const filename = join(directory, "notes.sqlite");
  const definition = createNotesFiles({
    filename,
    root: join(directory, "files"),
    principals: [
      { subjectId: "alice", key: "01".repeat(32) },
      { subjectId: "bob", key: "02".repeat(32) },
    ],
  });
  const plainWeb = createNotesWebPlugin(definition.notes, definition.authentication);
  await migrateFiles(filename);
  const running = await startApp({ plugins: [...definition.plugins, plainWeb] });
  try {
    const auth = running.get(definition.authentication);
    const service = running.get(definition.notes);
    const alice = (await auth.issue("01".repeat(32))).credential;
    const bob = (await auth.issue("02".repeat(32))).credential;
    const note = await service.create(await auth.for(notesAudiences.create).required(alice), {
      title: "Shared",
    });
    const router = createNotesRouter(service, auth);
    const api = createNotesOpenAPI(router);
    async function http(path: string, token?: string, input?: unknown) {
      const request = new Request(`https://notes.test${path}`, {
        method: input === undefined ? "GET" : "POST",
        headers: {
          ...(token ? { authorization: `Bearer ${token}` } : {}),
          "content-type": "application/json",
        },
        ...(input === undefined ? {} : { body: JSON.stringify(input) }),
      });
      return (await api.handle(request, { request, signal: request.signal }))!;
    }
    expect(
      (await running.get(plainWeb).fetch(new Request("https://notes.test/api/notes"))).status,
    ).toBe(404);
    const unauthenticated = await http("/api/notes");
    expect(unauthenticated.status).toBe(401);
    expect(unauthenticated.headers.get("content-type")).toBe("application/problem+json");
    expect((await unauthenticated.json()).code).toBe("UNAUTHORIZED");
    const invalid = await http("/api/notes/read", alice, { id: "PRIVATE-invalid-id" });
    expect(invalid.status).toBe(400);
    expect(await invalid.text()).not.toContain("PRIVATE-invalid-id");
    expect((await http("/api/notes/read", bob, { id: note.id })).status).toBe(403);
    expect((await http("/api/create", alice, { title: "not exposed" })).status).toBe(404);
    const spec = await api.generateSpec({
      info: { title: "Selected private Notes", version: "1.0.0" },
    });
    expect(Object.keys(spec.paths!)).toEqual(["/notes", "/notes/read"]);
    expect(spec.paths!["/notes/read"]!.post!.responses!["400"]).toHaveProperty(
      "content.application/problem+json",
    );
    const fetchAPI = (request: Request) =>
      api.handle(request, { request, signal: request.signal }).then((response) => response!);
    const openAPI = createORPCClient<RouterClient<typeof api.selectedRouter>>(
      new OpenAPILink(api.selectedRouter, {
        origin: "https://notes.test",
        url: "/api",
        headers: { authorization: `Bearer ${alice}` },
        customErrorResponseBodyDecoder: createProblemDetailsDecoder(),
        fetch: (url, init) => fetchAPI(new Request(url, init)),
      }),
    );
    expect(await openAPI.read({ id: note.id })).toEqual(note);
    const rpc = new RPCHandler(router);
    const native = createORPCClient<RouterClient<typeof router>>(
      new RPCLink({
        origin: "https://notes.test",
        url: "/rpc",
        headers: { authorization: `Bearer ${alice}` },
        fetch: async (url, init) => {
          const request = new Request(url, init);
          return (await rpc.handle(request, { prefix: "/rpc", context: { request } })).response!;
        },
      }),
    );
    expect(await native.read({ id: note.id })).toEqual(note);
  } finally {
    await running.stop();
    await rm(directory, { recursive: true, force: true });
  }
});
