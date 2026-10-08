import { definePlugin, type Plugin } from "lenso";
import type { SessionLifetime, SessionStore } from "@lenso/auth/sessions";
import { createNotesAuthPlugin, type NotesPrincipal } from "./auth";

/** Defer trusted environment configuration until setup, not static CLI inspection. */
export function createApplicationAuth<T>(options: {
  database: Plugin<T>;
  store(database: T): SessionStore<string>;
  principals: readonly NotesPrincipal[] | (() => readonly NotesPrincipal[]);
  lifetime?: SessionLifetime;
}) {
  return definePlugin({
    id: "notes-auth",
    requires: [options.database],
    setup(context) {
      return createNotesAuthPlugin({
        ...options,
        principals:
          typeof options.principals === "function" ? options.principals() : options.principals,
      }).setup(context);
    },
  });
}
