import { resolve } from "node:path";
import { defineApp, bindConfig, definePluginConfig, startApp } from "@lenso/core";
import {
  defineInitialization,
  initializationConfigBlockers,
  InitializationError,
} from "@lenso/core/initialization";
import { defineOperation } from "@lenso/cli";
import { z } from "zod";
import { createNativeFirstRunStore } from "./src/native";

// Trusted app configuration. Importing declares the graph; it opens no database.
export const store = createNativeFirstRunStore(
  resolve(process.env.LENSO_FIRST_RUN_HOME ?? `${import.meta.dir}/.state`),
);
const configuration = definePluginConfig({
  schema: z
    .object({
      owner: z.string().min(1),
      signingKey: z.string().min(32),
      providerKey: z.string().min(1),
    })
    .strict(),
  fields: [
    { path: ["signingKey"], sensitive: true },
    { path: ["providerKey"], sensitive: true },
  ],
});
export const greeting = bindConfig(
  configuration,
  [
    {
      descriptor: {
        id: "application-store",
        kind: "custom",
        fields: [
          { path: ["signingKey"], sensitive: true },
          { path: ["providerKey"], sensitive: true },
        ],
      },
      read: async () => ({ values: await store.configuration() }),
    },
  ],
  {
    id: "greeting",
    setup: (_context, config) => ({
      async greet() {
        return { message: `Hello, ${config.owner}!` };
      },
    }),
  },
);

// Native launcher authority is an opaque, short-lived in-process capability.
// Never mount initializationBinding as an HTTP route or accept a JSON "owner".
const localAuthorization = Object.freeze({});
const expiresAt = performance.now() + 10 * 60_000;
export function initializationBinding() {
  return localAuthorization;
}
export const initialization = defineInitialization({
  input: definePluginConfig({
    schema: z
      .object({
        username: z.string().trim().min(2).max(100),
        password: z.string().min(12).max(1024),
        providerKey: z.string().min(1).max(4096),
      })
      .strict(),
    fields: [
      { path: ["password"], sensitive: true },
      { path: ["providerKey"], sensitive: true },
    ],
  }),
  authorize: (_action, evidence) =>
    evidence === localAuthorization && performance.now() < expiresAt,
  read: store.read,
  commit: (request) =>
    store.commit(request, () => {
      if (performance.now() >= expiresAt)
        throw new InitializationError("initialization-unauthorized");
    }),
  check: () => initializationConfigBlockers([greeting]),
  activate: async () => {
    // Validate actual resource startup without calling business effects or an upstream.
    // The initialization fence is deliberately excluded from this private activation probe.
    const app = await startApp({ plugins: [greeting] });
    await app.stop();
  },
  complete: (revision) =>
    store.complete(revision, () => {
      if (performance.now() >= expiresAt)
        throw new InitializationError("initialization-unauthorized");
    }),
});
export const operations = [
  defineOperation({
    plugin: greeting,
    method: "greet",
    input: z.object({}).strict(),
    effect: "read",
    description: "Read the initialized local greeting; no provider calls.",
  }),
];
export default defineApp({ plugins: [greeting], initialization });
