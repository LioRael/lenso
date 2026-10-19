import { readFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

/** Installed package metadata, never a claim that matching versions equal a source revision. */
export function installedOperationVersions() {
  return Object.fromEntries(
    ["@lenso/core", "@lenso/engine", "@lenso/cli"].map((name) => {
      try {
        let dir = dirname(fileURLToPath(import.meta.resolve(name)));
        while (dirname(dir) !== dir) {
          const file = join(dir, "package.json");
          if (existsSync(file)) {
            const value = JSON.parse(readFileSync(file, "utf8"));
            if (value.name === name)
              return [name, typeof value.version === "string" ? value.version : null];
          }
          dir = dirname(dir);
        }
      } catch {
        /* Missing package metadata is unsupported, not an inferred version. */
      }
      return [name, null];
    }),
  );
}
