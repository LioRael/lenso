import { expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { isDocumentation, selectRoute, validateDocument } from "./ci-docs";

function fixture(run: (root: string, base: string) => void): void {
  const root = mkdtempSync(join(tmpdir(), "lenso-ci-docs-"));
  try {
    git(root, ["init", "-q"]);
    write(root, "README.md", "# Fixture\n");
    write(root, "source.ts", "export {};\n");
    const base = commit(root);
    run(root, base);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function git(root: string, args: string[]): string {
  const result = spawnSync("git", args, {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, GIT_EDITOR: "true", GIT_CONFIG_NOSYSTEM: "1" },
  });
  if (result.status !== 0) throw new Error(result.stderr);
  return result.stdout.trim();
}

function write(root: string, file: string, text = "# Documentation\n"): void {
  mkdirSync(dirname(join(root, file)), { recursive: true });
  writeFileSync(join(root, file), text);
}

function commit(root: string): string {
  git(root, ["add", "."]);
  git(root, [
    "-c",
    "user.name=Fixture",
    "-c",
    "user.email=fixture@example.invalid",
    "-c",
    "commit.gpgsign=false",
    "commit",
    "-qm",
    "Fixture",
  ]);
  return git(root, ["rev-parse", "HEAD"]);
}

test("narrow allowlist excludes unrelated Markdown and all runtime/config inputs", () => {
  for (const path of [
    "README.md",
    "AGENTS.md",
    "docs/a.md",
    ".agents/skills/a/SKILL.md",
    "packages/core/README.md",
    "examples/greeting/README.md",
  ])
    expect(isDocumentation(path)).toBe(true);
  for (const path of [
    "other.md",
    "tests/fixtures/a.md",
    ".changeset/a.md",
    "packages/core/src/a.md",
    "packages/core/CHANGELOG.md",
    "examples/x/src/README.md",
    ".github/workflows/checks.yml",
    "scripts/ci-docs.ts",
    "scripts/ci-docs.test.ts",
    "scripts/tsconfig.json",
    "package.json",
    "bun.lock",
    "mise.toml",
  ])
    expect(isDocumentation(path)).toBe(false);
});

test("complete PR/push diff allows ordinary docs and NUL-delimited spaced filenames", () => {
  fixture((root, base) => {
    write(root, "README.md", "# Changed\n");
    write(root, "docs/with space.md");
    commit(root);
    for (const event of ["pull_request", "push"])
      expect(selectRoute(root, { event, base })).toEqual({
        route: "light",
        files: ["README.md", "docs/with space.md"],
      });
  });
});

test("each unknown/runtime/config path and mixed diff goes full", () => {
  for (const path of [
    "other.md",
    "tests/fixtures/a.md",
    ".changeset/a.md",
    ".github/workflows/checks.yml",
    "scripts/ci-docs.ts",
    "scripts/ci-docs.test.ts",
    "scripts/tsconfig.json",
    "package.json",
    "bun.lock",
    "mise.toml",
    "source.ts",
  ]) {
    fixture((root, base) => {
      write(root, "docs/new.md");
      write(root, path, "changed\n");
      commit(root);
      expect(selectRoute(root, { event: "pull_request", base }).route).toBe("full");
    });
  }
});

test("source-to-doc rename, deletion, executable mode, symlink and tree changes go full", () => {
  for (const change of ["rename", "delete", "executable", "symlink", "tree"]) {
    fixture((root, base) => {
      if (change === "rename") {
        mkdirSync(join(root, "docs"));
        git(root, ["mv", "source.ts", "docs/source.md"]);
      } else if (change === "delete") {
        rmSync(join(root, "README.md"));
      } else if (change === "executable") {
        chmodSync(join(root, "README.md"), 0o755);
      } else if (change === "symlink") {
        mkdirSync(join(root, "docs"));
        symlinkSync("../README.md", join(root, "docs/link.md"));
      } else {
        rmSync(join(root, "README.md"));
        write(root, "README.md/child.md");
      }
      commit(root);
      expect(selectRoute(root, { event: "push", base }).route).toBe("full");
    });
  }
});

test("dispatch, initial/missing/invalid/unavailable base, empty diff and Git failure go full", () => {
  fixture((root, base) => {
    expect(selectRoute(root, { event: "push", base }).route).toBe("full");
    write(root, "docs/new.md");
    commit(root);
    for (const inputs of [
      { event: "workflow_dispatch", base },
      { event: "unknown", base },
      { event: "push" },
      { event: "push", base: "0".repeat(40) },
      { event: "push", base: "--unsafe" },
      { event: "push", base: "a".repeat(40) },
    ])
      expect(selectRoute(root, inputs).route).toBe("full");
    expect(selectRoute(join(root, "absent"), { event: "push", base }).route).toBe("full");
    expect(selectRoute(root, { event: "push", base: "a".repeat(40) }, true).route).toBe("full");
  });
});

test("a shallow checkout fetches only the event base and safely falls back without it", () => {
  fixture((root, base) => {
    write(root, "docs/new.md");
    commit(root);
    const shallow = mkdtempSync(join(tmpdir(), "lenso-ci-docs-shallow-"));
    try {
      git(root, ["clone", "--depth=1", `file://${root}`, shallow]);
      expect(selectRoute(shallow, { event: "pull_request", base }).route).toBe("full");
      expect(selectRoute(shallow, { event: "pull_request", base }, true).route).toBe("light");
    } finally {
      rmSync(shallow, { recursive: true, force: true });
    }
  });
});

test("docs validator checks local files, skill YAML and fences, not fenced examples or external URLs", () => {
  fixture((root) => {
    write(root, "docs/space name.md");
    write(root, "docs/parentheses(a).md");
    const skill = "---\nname: fixture\ndescription: >-\n  Fixture skill\n---\n";
    validateDocument(root, ".agents/skills/fixture/SKILL.md", skill);
    validateDocument(
      root,
      "docs/a.md",
      "[readme](../README.md#section)\n[space](<space name.md>)\n[ref]: ../README.md\n[external](https://does-not-exist.invalid/a)\n```md\n[example](missing.md)\n---\nname: [bad\n```\n",
    );
    validateDocument(
      root,
      "docs/a.md",
      "[nested](parentheses(a).md)\n> ```md\n> [example](missing.md)\n> ```\n    [indented example](missing.md)\n`[inline example](missing.md)`\n",
    );
    validateDocument(root, "docs/reference.md", "---\nname: [reference prose\n---\n");
    validateDocument(root, "docs/SKILL.md", "# Skill reference prose\n");
    expect(() => validateDocument(root, "docs/a.md", "[missing](missing.md)")).toThrow(
      "Missing local link",
    );
    expect(() => validateDocument(root, "docs/a.md", "[missing]: missing.md")).toThrow(
      "Missing local link",
    );
    expect(() => validateDocument(root, "docs/a.md", "```ts\nexport {};\n")).toThrow("Unclosed");
    expect(() => validateDocument(root, "docs/a.md", "~~~\n```\n")).toThrow("Unclosed");
    expect(() =>
      validateDocument(root, ".agents/skills/x/SKILL.md", "---\nname: [bad\n---\n"),
    ).toThrow();
    expect(() =>
      validateDocument(root, ".agents/skills/x/SKILL.md", "---\nname: x\n---\n"),
    ).toThrow("requires");
    expect(() => validateDocument(root, ".agents/skills/x/SKILL.md", "# No frontmatter\n")).toThrow(
      "frontmatter",
    );
  });
});

test("indented fence examples are prose-safe while actual unmatched fences fail", () => {
  fixture((root) => {
    for (const text of [
      "    ```\n    literal example\n",
      "\t```\n\tliteral example\n",
      ">     ```\n>     literal example\n",
    ])
      expect(() => validateDocument(root, "docs/a.md", text)).not.toThrow();
    for (const text of [
      "```\nliteral example\n",
      "   ```\nliteral example\n",
      "> ```\n> literal example\n",
      "```\n    ```\n",
    ])
      expect(() => validateDocument(root, "docs/a.md", text)).toThrow("Unclosed code fence");
  });
});

test("nested and escaped bracket labels still validate their local destinations", () => {
  fixture((root) => {
    for (const label of [
      "CLI [inspect]",
      "CLI [nested [inspect]]",
      "CLI \\[inspect\\]",
      "CLI \\]inspect",
      "CLI \\[inspect",
    ]) {
      expect(() => validateDocument(root, "docs/a.md", `[${label}](missing.md)`)).toThrow(
        "Missing local link",
      );
      expect(() => validateDocument(root, "docs/a.md", `[${label}](../README.md)`)).not.toThrow();
    }
    expect(() =>
      validateDocument(root, "docs/a.md", "\\[escaped opener](missing.md)"),
    ).not.toThrow();
  });
});

test("actual CLI uses workflow inputs: docs light, malformed docs failure, source/mixed full", () => {
  fixture((root, base) => {
    const cli = resolve(import.meta.dir, "ci-docs.ts");
    const run = (command: string) =>
      spawnSync(process.execPath, [cli, command], {
        cwd: root,
        encoding: "utf8",
        env: {
          ...process.env,
          CI_EVENT: "pull_request",
          CI_BASE: base,
          GITHUB_OUTPUT: join(root, "output"),
        },
      });
    write(root, ".git/info/exclude", "node_modules\noutput\n");
    write(root, "docs/valid.md", "# Valid\n\n[Readme](../README.md)\n");
    commit(root);
    symlinkSync(resolve(import.meta.dir, "../node_modules"), join(root, "node_modules"));
    expect(run("select").stdout.trim()).toBe("light");
    const valid = run("check");
    expect(valid.status).toBe(0);
    rmSync(join(root, "output"));
    write(root, "docs/broken.md", "```\nunclosed\n");
    commit(root);
    const docs = run("select");
    expect(docs.status).toBe(0);
    expect(docs.stdout.trim()).toBe("light");
    expect(readFileSync(join(root, "output"), "utf8")).toBe("route=light\n");
    const broken = run("check");
    expect(broken.status).toBe(1);
    expect(broken.stderr).toContain("Unclosed code fence");
    write(root, "source.ts", "export const changed = true;\n");
    commit(root);
    expect(run("select").stdout.trim()).toBe("full");
    console.log(
      "CLI fixture evidence: docs -> light; valid docs check -> exit 0; unclosed fence check -> exit 1; mixed -> full",
    );
  });
});

test("actual CLI selects full for a source-only push", () => {
  fixture((root, base) => {
    write(root, "source.ts", "export const changed = true;\n");
    commit(root);
    const result = spawnSync(process.execPath, [resolve(import.meta.dir, "ci-docs.ts"), "select"], {
      cwd: root,
      encoding: "utf8",
      env: { ...process.env, CI_EVENT: "push", CI_BASE: base, GITHUB_OUTPUT: "" },
    });
    expect(result.status).toBe(0);
    expect(result.stdout.trim()).toBe("full");
    console.log("CLI fixture evidence: source-only push -> full");
  });
});

test("workflow retains required job/events/full commands and unconditional frozen install/test gate", () => {
  const yaml = Bun.YAML.parse(
    readFileSync(resolve(import.meta.dir, "../.github/workflows/checks.yml"), "utf8"),
  ) as {
    on: Record<string, unknown>;
    jobs: Record<string, { if?: string; steps: Array<Record<string, any>> }>;
  };
  expect(Object.keys(yaml.on)).toEqual(["pull_request", "push", "workflow_dispatch"]);
  expect(yaml.on.push).toEqual({ branches: ["main"] });
  expect(yaml.on.pull_request).toBeNull();
  const job = yaml.jobs.checks!;
  expect(Object.keys(yaml.jobs)).toEqual(["checks"]);
  expect(job.if).toBeUndefined();
  for (const command of ["bun install --frozen-lockfile", "bun test scripts/ci-docs.test.ts"]) {
    const step = job.steps.find((candidate) => candidate.run === command)!;
    expect(step).toBeDefined();
    expect(step.if).toBeUndefined();
  }
  const fullSteps = job.steps.filter((step) => step.if === "steps.scope.outputs.route != 'light'");
  expect(fullSteps).toHaveLength(3);
  expect(fullSteps.some((step) => step.with?.["node-version"] === "24.21.0")).toBe(true);
  expect(fullSteps.some((step) => step.run?.includes("apt-get install --yes postgresql-16"))).toBe(
    true,
  );
  expect(fullSteps.some((step) => step.run === "bash scripts/ci-checks.sh")).toBe(true);
  const docs = job.steps.find((step) => step.run === "bun scripts/ci-docs.ts check")!;
  expect(docs.if).toBe("steps.scope.outputs.route == 'light'");
  expect(docs["continue-on-error"]).toBeUndefined();
  expect(job.steps.find((step) => step.id === "scope")!.env.CI_BASE).toBe(
    "${{ github.event.pull_request.base.sha || github.event.before }}",
  );
});
