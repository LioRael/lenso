import { startApp } from "lenso";
import { createPgNotesPlugins, databaseUrl } from "./app-pg";
import { AuthConfigurationError, AuthError } from "@lenso/auth";
import { parseNotesPrincipals, type NotesAuthentication } from "./auth";
import { NoteInputError, notesAudiences, type NotesService } from "./notes";

export async function runNotesCommand(
  service: NotesService,
  authentication: NotesAuthentication,
  args: readonly string[],
  credential: string | null,
): Promise<unknown> {
  const [command, first, second, third] = args;
  switch (command) {
    case "create":
      if (first === undefined) break;
      return service.create(await authentication.for(notesAudiences.create).required(credential), {
        title: first,
        body: second,
      });
    case "list":
      return service.list(await authentication.for(notesAudiences.list).required(credential));
    case "read":
      if (first === undefined) break;
      return service.read(
        await authentication.for(notesAudiences.read).required(credential),
        first,
      );
    case "update":
      if (first === undefined || second === undefined) break;
      return service.update(
        await authentication.for(notesAudiences.update).required(credential),
        first,
        { title: second, body: third },
      );
    case "remove":
      if (first === undefined) break;
      return {
        removed: await service.remove(
          await authentication.for(notesAudiences.remove).required(credential),
          first,
        ),
      };
  }
  throw new NoteInputError(
    'Usage: cli create "title" ["body"] | list | read <id> | update <id> "title" ["body"] | remove <id> | login | renew | revoke',
  );
}

if (import.meta.main) {
  try {
    const definition = createPgNotesPlugins(
      databaseUrl(),
      parseNotesPrincipals(process.env.NOTES_LOGIN_KEYS),
    );
    const app = await startApp(definition);
    try {
      const authentication = app.get(definition.authentication);
      const args = process.argv.slice(2);
      const credential = process.env.NOTES_SESSION ?? null;
      let result: unknown;
      if (args[0] === "login") {
        const key = process.env.NOTES_LOGIN_KEY;
        if (!key) throw new AuthError("UNAUTHORIZED");
        result = await authentication.issue(key);
      } else if (args[0] === "renew" || args[0] === "revoke") {
        if (!credential) throw new AuthError("UNAUTHORIZED");
        if (args[0] === "renew") result = await authentication.renew(credential);
        else {
          await authentication.revoke(credential);
          result = { revoked: true };
        }
      } else {
        result = await runNotesCommand(app.get(definition.notes), authentication, args, credential);
      }
      console.log(JSON.stringify(result, null, 2));
    } finally {
      await app.stop();
    }
  } catch (error) {
    const safe = error instanceof AuthError ? new AuthError(error.code) : null;
    const code =
      safe?.code ??
      (error instanceof NoteInputError
        ? "BAD_REQUEST"
        : error instanceof AuthConfigurationError
          ? "CONFIGURATION_ERROR"
          : "SERVICE_UNAVAILABLE");
    console.error(
      JSON.stringify({
        code,
        message:
          safe?.message ??
          (error instanceof NoteInputError ? error.message : "Notes command unavailable"),
      }),
    );
    process.exitCode = 1;
  }
}
