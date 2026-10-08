import { AuthConfigurationError, createAuth, defineSource, realm, type Auth } from "@lenso/auth";
import {
  createManagedSessions,
  type ManagedSession,
  type SessionLifetime,
  type SessionStore,
} from "@lenso/auth/sessions";
import { definePlugin, type Plugin } from "@lenso/core";
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
    return principalsSchema.parse(JSON.parse(value ?? ""));
  } catch {
    throw new AuthConfigurationError(
      "Configure NOTES_LOGIN_KEYS as unique subjects and 32-byte hex keys",
    );
  }
}

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
  principals: readonly NotesPrincipal[];
  lifetime?: SessionLifetime;
}): Plugin<NotesAuthentication> {
  const validated = principalsSchema.safeParse(options.principals);
  if (!validated.success) throw new AuthConfigurationError("Invalid Notes login configuration");
  const principals = validated.data;
  return definePlugin({
    id: options.id ?? "notes-auth",
    requires: [options.database],
    async setup(context) {
      const configured = await Promise.all(
        principals.map(async ({ subjectId, key }) => ({
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
        lifetime: options.lifetime ?? { idle: 3_600_000, absolute: 86_400_000, renewAfter: 60_000 },
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
