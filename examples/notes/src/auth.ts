import { AuthConfigurationError, createAuth, defineSource, realm, type Auth } from "@lenso/auth";
import {
  createManagedSessions,
  sessionLifetime,
  type ManagedSession,
  type SessionLifetime,
  type SessionStore,
} from "@lenso/auth/sessions";
import type { Plugin } from "@lenso/core";
import {
  bindConfig,
  definePluginConfig,
  valuesSource,
  type ConfigSource,
} from "@lenso/core/config";
import { z } from "zod";

export interface NotesPrincipal {
  subjectId: string;
  key: string;
}

const principalsSchema = z
  .array(
    z.strictObject({
      subjectId: z
        .string()
        .trim()
        .min(1)
        .max(512)
        .refine((value) => value !== "__legacy_unowned__"),
      key: z.string().regex(/^[0-9a-fA-F]{64}$/),
    }),
  )
  .min(1)
  .refine((values) => new Set(values.map((value) => value.subjectId)).size === values.length)
  .refine(
    (values) => new Set(values.map((value) => value.key.toLowerCase())).size === values.length,
  );

export function parseNotesPrincipals(value: string | undefined): NotesPrincipal[] {
  try {
    return principalsInputSchema.parse(value);
  } catch {
    throw new AuthConfigurationError(
      "Configure NOTES_LOGIN_KEYS as unique subjects and 32-byte hex keys",
    );
  }
}

const principalsInputSchema = z.preprocess((value) => {
  if (typeof value !== "string") return value;
  try {
    return JSON.parse(value);
  } catch {
    return undefined;
  }
}, principalsSchema);

export const notesAuthConfig = definePluginConfig({
  description: "Notes login principals and managed session lifetime",
  fields: [{ path: ["principals"], sensitive: true }],
  schema: z
    .strictObject({
      principals: principalsInputSchema,
      idle: z.number().default(3_600_000),
      absolute: z.number().default(86_400_000),
      renewAfter: z.number().default(60_000),
    })
    .transform((value, context) => {
      try {
        return { principals: value.principals, lifetime: sessionLifetime(value) };
      } catch {
        context.addIssue({
          code: "custom",
          path: ["renewAfter"],
          message: "Invalid session lifetime",
        });
        return z.NEVER;
      }
    }),
});

export type NotesPrincipalsInput =
  | readonly NotesPrincipal[]
  | string
  | (() => readonly NotesPrincipal[] | string | undefined)
  | { sources: readonly ConfigSource[] };

export interface NotesAuthentication extends Auth<"notes", string | null, string> {
  issue(key: string, options?: { signal?: AbortSignal }): Promise<ManagedSession>;
  renew(credential: string, options?: { signal?: AbortSignal }): Promise<ManagedSession>;
  revoke(credential: string, options?: { signal?: AbortSignal }): Promise<void>;
}

async function digest(key: string): Promise<Uint8Array> {
  return new Uint8Array(
    await crypto.subtle.digest("SHA-256", new TextEncoder().encode(key.toLowerCase())),
  );
}

export function createNotesAuthPlugin<T>(options: {
  id?: string;
  database: Plugin<T>;
  store(database: T): SessionStore<string>;
  principals: NotesPrincipalsInput;
  lifetime?: SessionLifetime;
}): Plugin<NotesAuthentication> {
  const input = options.principals;
  const lifetime = options.lifetime;
  const sources: readonly ConfigSource[] =
    typeof input === "object" && input !== null && "sources" in input
      ? [
          ...input.sources,
          ...(lifetime ? [valuesSource({ ...lifetime }, { id: "notes-lifetime" })] : []),
        ]
      : typeof input === "function"
        ? [
            {
              descriptor: {
                id: "notes-principals",
                kind: "supplier",
                fields: [{ path: ["principals"], sensitive: true }],
              },
              async read() {
                return { values: { principals: input(), ...lifetime } };
              },
            },
          ]
        : [valuesSource({ principals: input, ...lifetime }, { sensitive: [["principals"]] })];
  return bindConfig(notesAuthConfig, sources, {
    id: options.id ?? "notes-auth",
    requires: [options.database],
    async setup(context, config) {
      const configured = await Promise.all(
        config.principals.map(async ({ subjectId, key }) => ({
          subjectId,
          digest: await digest(key),
        })),
      );
      const login = defineSource<string, string>({
        realmId: "notes",
        async verify(key, { signal }) {
          signal.throwIfAborted();
          if (typeof key !== "string" || !/^[0-9a-fA-F]{64}$/.test(key))
            return { status: "rejected" };
          const supplied = await digest(key);
          let subjectId: string | undefined;
          for (const principal of configured) {
            let difference = 0;
            for (let i = 0; i < supplied.length; i++)
              difference |= supplied[i]! ^ principal.digest[i]!;
            if (difference === 0) subjectId = principal.subjectId;
          }
          signal.throwIfAborted();
          return subjectId === undefined
            ? { status: "rejected" }
            : { status: "verified", subjectId, kind: "user" };
        },
      });
      const sessions = createManagedSessions({
        realmId: "notes",
        login,
        store: options.store(context.get(options.database)),
        lifetime: config.lifetime,
        subjectActive: async (subject) =>
          configured.some((principal) => principal.subjectId === subject),
      });
      context.onCleanup(() => sessions.close());
      const authentication = createAuth(realm("notes", sessions.source));
      context.onCleanup(() => authentication.close());
      return {
        ...authentication,
        issue: sessions.issue,
        renew: sessions.renew,
        revoke: sessions.revoke,
      };
    },
  });
}
