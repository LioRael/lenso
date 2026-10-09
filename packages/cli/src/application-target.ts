import { stat } from "node:fs/promises";
import { isAbsolute, relative, resolve, sep } from "node:path";
import { type ApplicationTarget } from "@lenso/engine/application";
import { CliError } from "./diagnostics";

/** Workspace discovery suggests a selection; it never imports or authorizes an application. */
export async function selectApplication(
  root: string,
  app?: string,
  config?: string,
): Promise<ApplicationTarget> {
  const base = resolve(root);
  const selected = app === undefined ? base : resolve(base, app);
  const local = relative(base, selected);
  if (
    app !== undefined &&
    (!app || isAbsolute(app) || local === ".." || local.startsWith(`..${sep}`) || isAbsolute(local))
  )
    throw new CliError(
      {
        code: "invalid-application-target",
        phase: "discovery",
        message: "--app must be an application directory inside --root.",
      },
      3,
    );
  if (app !== undefined) {
    if (!(await stat(selected).catch(() => undefined))?.isDirectory())
      throw new CliError(
        {
          code: "invalid-application-target",
          phase: "discovery",
          message: "--app must name an existing application directory inside --root.",
          source: { file: selected },
        },
        3,
      );
  } else if (config === undefined && !(await Bun.file(resolve(base, "lenso.config.ts")).exists())) {
    const manifest = Bun.file(resolve(base, "package.json"));
    if (await manifest.exists()) {
      let workspaces: unknown;
      try {
        const loaded = await manifest.json();
        workspaces = Array.isArray(loaded.workspaces)
          ? loaded.workspaces
          : loaded.workspaces?.packages;
      } catch {
        // A malformed package manifest is not an application registry.
      }
      if (Array.isArray(workspaces) && workspaces.every((pattern) => typeof pattern === "string")) {
        const candidates = new Set<string>();
        const exclusions = workspaces
          .filter((pattern) => pattern.startsWith("!"))
          .map((pattern) => new Bun.Glob(pattern.slice(1)));
        for (const pattern of workspaces.filter((workspace) => !workspace.startsWith("!"))) {
          if (isAbsolute(pattern) || pattern.split(/[\\/]/).includes("..")) continue;
          for await (const directory of new Bun.Glob(pattern).scan({
            cwd: base,
            onlyFiles: false,
          })) {
            const path = resolve(base, directory);
            const candidate = relative(base, path);
            if (
              !candidate ||
              candidate === ".." ||
              candidate.startsWith(`..${sep}`) ||
              isAbsolute(candidate) ||
              exclusions.some((glob) => glob.match(directory)) ||
              !(await stat(path).catch(() => undefined))?.isDirectory()
            )
              continue;
            if (await Bun.file(resolve(path, "lenso.config.ts")).exists())
              candidates.add(candidate.split(sep).join("/"));
          }
        }
        const directories = [...candidates].sort();
        throw new CliError(
          {
            code:
              directories.length > 1
                ? "ambiguous-application-target"
                : "missing-application-selection",
            phase: "discovery",
            message: directories.length
              ? "Select an application with --app <directory> from the workspace candidates, or provide --root/--config explicitly."
              : "No canonical application config in declared workspaces. Provide --app <directory> and --config <file>, or an explicit --root/--config.",
            source: { file: resolve(base, "package.json") },
            details: { candidates: directories },
          },
          3,
        );
      }
    }
  }
  return { root: selected, ...(config === undefined ? {} : { config }) };
}
