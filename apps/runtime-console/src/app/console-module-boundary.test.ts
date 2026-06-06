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
const storyModulePrefix = "../modules/story-console/";
const importPattern =
  /\b(?:import|export)\s+(?:type\s+)?(?:[^'"]*?\s+from\s+)?["']([^"']+)["']/g;

function findConsoleModuleBoundaryViolations(): string[] {
  const violations: string[] = [];

  for (const [file, source] of Object.entries(sourceFiles)) {
    const inStoryModule = file.startsWith(storyModulePrefix);

    for (const specifier of importSpecifiers(source)) {
      const target = resolveImport(file, specifier);

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
        target.startsWith(storyModulePrefix) &&
        target !== "../modules/story-console"
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
