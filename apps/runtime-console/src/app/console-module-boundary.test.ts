import { describe, expect, test } from "vitest";

describe("console module boundaries", () => {
  test("keeps story-console internals behind the module entrypoint", () => {
    expect(findConsoleModuleBoundaryViolations()).toEqual([]);
  });
});

const sourceFiles = import.meta.glob<string>("../**/*.{ts,tsx}", {
  eager: true,
  import: "default",
  query: "?raw",
});
const storyPackageFiles = import.meta.glob<string>(
  "../../packages/story-console/src/**/*.{ts,tsx}",
  {
    eager: true,
    import: "default",
    query: "?raw",
  }
);
const modulePrefix = "../modules/";
const storyPackagePrefix = "../../packages/story-console/src/";
const storyModulePrefix = "../modules/story-console/";
const importPattern =
  /\b(?:import|export)\s+(?:type\s+)?(?:[^'"]*?\s+from\s+)?["']([^"']+)["']/g;

function findConsoleModuleBoundaryViolations(): string[] {
  const violations: string[] = [];

  for (const [file, source] of Object.entries({
    ...sourceFiles,
    ...storyPackageFiles,
  })) {
    const inConsoleModule =
      file.startsWith(modulePrefix) || file.startsWith(storyPackagePrefix);
    const inStoryModule =
      file.startsWith(storyModulePrefix) || file.startsWith(storyPackagePrefix);

    for (const specifier of importSpecifiers(source)) {
      const target = resolveImport(file, specifier);

      if (inConsoleModule && target.includes("/app/")) {
        violations.push(
          `${displayPath(file)} imports host app internals through ${specifier}`
        );
      }

      if (
        inConsoleModule &&
        target === "@lenso/runtime-console/console-package-api"
      ) {
        violations.push(
          `${displayPath(file)} imports host API through ${specifier}; use @lenso/runtime-console-api`
        );
      }

      if (inStoryModule && target.includes("/pages/")) {
        violations.push(
          `${displayPath(file)} imports host pages through ${specifier}`
        );
      }

      if (inStoryModule && target.includes("/hooks/")) {
        violations.push(
          `${displayPath(file)} imports host hooks through ${specifier}`
        );
      }

      if (inStoryModule && target.endsWith("/runtime-console-context")) {
        violations.push(
          `${displayPath(file)} imports host runtime context through ${specifier}`
        );
      }

      if (inStoryModule && target.includes("/components/runtime/")) {
        violations.push(
          `${displayPath(file)} imports host runtime UI through ${specifier}`
        );
      }

      if (inStoryModule && target.includes("/components/ui/")) {
        violations.push(
          `${displayPath(file)} imports host common UI through ${specifier}`
        );
      }

      if (inStoryModule && target.includes("/data/")) {
        violations.push(
          `${displayPath(file)} imports host data through ${specifier}`
        );
      }

      if (
        !inStoryModule &&
        (target === "@lenso/story-console" ||
          target === "../modules/story-console" ||
          target.startsWith(storyModulePrefix)) &&
        target !== "@lenso/story-console"
      ) {
        violations.push(
          `${displayPath(file)} imports story-console internals through ${specifier}`
        );
      }
    }
  }

  return violations.sort();
}

function importSpecifiers(source: string): string[] {
  return [...source.matchAll(importPattern)].map((match) => match[1] ?? "");
}

function resolveImport(importer: string, specifier: string): string {
  if (!specifier.startsWith(".")) {
    return specifier;
  }

  const importerParts = importer.split("/");
  importerParts.pop();
  for (const part of specifier.split("/")) {
    if (part === "." || part === "") {
      continue;
    }
    if (part === "..") {
      importerParts.pop();
      continue;
    }
    importerParts.push(part);
  }
  return importerParts.join("/");
}

function displayPath(path: string): string {
  return path.replace(/^\.\.\//u, "");
}
