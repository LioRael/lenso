import { randomBytes, scrypt, timingSafeEqual } from "node:crypto";
import { z } from "zod";
import {
  ActionError, Catalog, ConsoleExtension, Executor, Module, defineAction,
  emptyInput, requireOwner, requirePrincipal, requireScope,
} from "@lenso/app";
import type { Context } from "@lenso/app";
import { Store, fileInfo, userInfo } from "./store.js";

const credentialsInput = z.object({
  name: z.string().regex(/^[a-zA-Z0-9_-]{3,64}$/),
  password: z.string().refine((value) => Buffer.byteLength(value) >= 12 && Buffer.byteLength(value) <= 128),
});
export const uploadWireInput = z.object({ name: z.string(), content: z.string() });

function passwordHash(password: string, salt: string): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    scrypt(password, salt, 64, { N: 16384, r: 8, p: 1 }, (error, key) => {
      if (error) reject(error); else resolve(key);
    });
  });
}

export function createApplication(store: Store) {
  const action = defineAction<Store>();
  const executor = new Executor((record) => {
    try { store.record(record); }
    catch (error) { console.error("Could not persist call record", error); }
  });
  const register = action({
    id: "auth.register", input: credentialsInput, output: userInfo,
    authorize() {},
    async run(context, input) {
      const salt = randomBytes(16).toString("hex");
      const hash = await passwordHash(input.password, salt);
      return context.resources.addUser(input.name, salt, hash.toString("hex"));
    },
  });
  const login = action({
    id: "auth.login", input: credentialsInput, output: z.object({ token: z.string() }),
    authorize() {},
    async run(context, input) {
      const credentials = context.resources.credentials(input.name);
      const hash = await passwordHash(input.password, credentials?.salt ?? "missing-user-salt");
      if (!credentials || !timingSafeEqual(hash, Buffer.from(credentials.hash, "hex"))) {
        throw new ActionError("unauthenticated", "Invalid credentials");
      }
      return { token: context.resources.session(credentials.id) };
    },
  });
  const upload = action({
    id: "files.upload",
    input: z.object({ name: z.string().min(1).max(255), content: z.instanceof(Uint8Array).refine((value) => value.byteLength <= 1_048_576) }),
    output: fileInfo,
    authorize(context) { requireScope(context.principal, "files:write"); },
    run(context, input) { return context.resources.upload(requirePrincipal(context.principal).subject, input.name, input.content); },
  });
  const read = action({
    id: "files.read", input: z.object({ id: z.string() }), output: fileInfo.extend({ content: z.instanceof(Uint8Array) }),
    authorize(context, input) {
      requireScope(context.principal, "files:read");
      requireOwner(context.principal, context.resources.read(input.id).owner);
    },
    run(context, input) { return context.resources.read(input.id); },
  });
  const list = action({
    id: "files.list", input: emptyInput, output: z.array(fileInfo),
    authorize(context) { requireScope(context.principal, "files:read"); },
    run(context) { return context.resources.list(requirePrincipal(context.principal).subject); },
  });
  const users = action({
    id: "console.users", input: emptyInput, output: z.array(userInfo),
    authorize(context) { requireScope(context.principal, "console:read"); },
    run(context) { return context.resources.users(); },
  });
  const files = action({
    id: "console.files", input: emptyInput, output: z.array(fileInfo),
    authorize(context) { requireScope(context.principal, "console:read"); },
    run(context) { return context.resources.files(); },
  });
  const requests = action({
    id: "console.requests", input: emptyInput,
    authorize(context) { requireScope(context.principal, "console:read"); },
    run(context) { return context.resources.requests(); },
  });
  const consoleExtension = new ConsoleExtension(executor, [
    { id: "users", title: "Users", list: users },
    { id: "files", title: "Files", list: files },
    { id: "requests", title: "Requests", list: requests },
  ]);
  const catalog = new Catalog();
  const manifest = action({
    id: "console.manifest", input: emptyInput,
    authorize(context) { requireScope(context.principal, "console:read"); },
    run() { return consoleExtension.manifest(catalog); },
  });
  catalog.register(new Module("auth", { actions: { register, login }, resources: ["sqlite"] }));
  catalog.register(new Module("files", { actions: { upload, read, list }, resources: ["sqlite"] }));
  catalog.register(new Module("console", { actions: { users, files, requests, manifest }, resources: ["sqlite"] }));
  catalog.requireResources(["sqlite"]);

  function context(token?: string, signal = new AbortController().signal): Context<Store> {
    return { callId: randomBytes(16).toString("hex"), principal: store.authenticate(token), resources: store, signal };
  }
  return { executor, actions: { register, login, upload, read, list }, consoleExtension, manifest, catalog, context };
}
