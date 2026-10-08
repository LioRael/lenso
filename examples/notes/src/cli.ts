import { startApp } from "lenso";
import { createPgNotesPlugins, databaseUrl } from "./app-pg";
import type { NotesService } from "./notes";

export async function runNotesCommand(
  service: NotesService,
  args: readonly string[],
): Promise<unknown> {
  const [command, first, second, third] = args;
  switch (command) {
    case "create":
      if (first === undefined) break;
      return service.create({ title: first, body: second });
    case "list":
      return service.list();
    case "update":
      if (first === undefined || second === undefined) break;
      return service.update(first, { title: second, body: third });
    case "remove":
      if (first === undefined) break;
      return { removed: await service.remove(first) };
  }
  throw new Error(
    'Usage: cli create "title" ["body"] | list | update <id> "title" ["body"] | remove <id>',
  );
}

if (import.meta.main) {
  const definition = createPgNotesPlugins(databaseUrl());
  const app = await startApp(definition);
  try {
    console.log(
      JSON.stringify(
        await runNotesCommand(app.get(definition.notes), process.argv.slice(2)),
        null,
        2,
      ),
    );
  } finally {
    await app.stop();
  }
}
