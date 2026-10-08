import { startApp } from "@lenso/core";
import { call, CliError } from "@lenso/cli";
import { AuthConfigurationError, AuthError } from "@lenso/auth";
import { fileURLToPath } from "node:url";
import { NoteInputError } from "./notes";

if (import.meta.main) {
  try {
    const [command, json, ...extra] = process.argv.slice(2);
    if (!command || extra.length)
      throw new NoteInputError("Use <method> [JSON input] or login | renew | revoke");
    let result: unknown;
    if (["login", "renew", "revoke"].includes(command)) {
      const configUrl = new URL("../lenso.config.ts", import.meta.url);
      const configuration: typeof import("../lenso.config") = await import(configUrl.href);
      const { default: definition, definition: services } = configuration;
      const app = await startApp(definition);
      try {
        const authentication = app.get(services.authentication);
        const credential = process.env.NOTES_SESSION ?? null;
        if (command === "login") {
          const key = process.env.NOTES_LOGIN_KEY;
          if (!key) throw new AuthError("UNAUTHORIZED");
          result = await authentication.issue(key);
        } else {
          if (!credential) throw new AuthError("UNAUTHORIZED");
          if (command === "renew") result = await authentication.renew(credential);
          else {
            await authentication.revoke(credential);
            result = { revoked: true };
          }
        }
      } finally {
        await app.stop();
      }
    } else {
      let input: unknown;
      try {
        input = JSON.parse(json ?? (await Bun.stdin.text()));
      } catch {
        throw new NoteInputError("Provide one JSON business input, using stdin for private data");
      }
      result = await call(
        fileURLToPath(new URL("../", import.meta.url)),
        "notes-operations",
        command,
        input,
      );
    }
    console.log(JSON.stringify(result, null, 2));
  } catch (error) {
    const safe = error instanceof AuthError ? new AuthError(error.code) : null;
    console.error(
      JSON.stringify({
        code:
          safe?.code ??
          (error instanceof CliError
            ? error.diagnostic.code
            : error instanceof NoteInputError
              ? "BAD_REQUEST"
              : error instanceof AuthConfigurationError
                ? "CONFIGURATION_ERROR"
                : "SERVICE_UNAVAILABLE"),
        message:
          safe?.message ??
          (error instanceof CliError
            ? error.diagnostic.message
            : error instanceof NoteInputError
              ? error.message
              : "Notes command unavailable"),
      }),
    );
    process.exitCode = 1;
  }
}
