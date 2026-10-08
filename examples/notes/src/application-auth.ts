import type { Plugin } from "@lenso/core";
import type { SessionLifetime, SessionStore } from "@lenso/auth/sessions";
import { createNotesAuthPlugin, type NotesPrincipalsInput } from "./auth";

export function createApplicationAuth<T>(options: {
  database: Plugin<T>;
  store(database: T): SessionStore<string>;
  principals: NotesPrincipalsInput;
  lifetime?: SessionLifetime;
}) {
  return createNotesAuthPlugin(options);
}
