import { spawnSync } from "node:child_process";
import { appendFileSync, existsSync, readFileSync, realpathSync } from "node:fs";
import { dirname, isAbsolute, relative, resolve } from "node:path";

type Inputs = { event?: string; base?: string };
export type Route = { route: "light" | "full"; files: string[] };
const full: Route = { route: "full", files: [] };

export function isDocumentation(path: string): boolean {
  return (
    /^(README|AGENTS)\.md$/.test(path) ||
    /^(docs|\.agents)\/[^\\\0]+\.md$/.test(path) ||
    /^(packages|examples)\/[^/\\]+\/README\.md$/.test(path)
  );
}

function git(root: string, args: string[]): string {
  const result = spawnSync("git", args, { cwd: root, encoding: "utf8" });
  if (result.error || result.status !== 0) throw new Error("Git command failed");
  return result.stdout;
}

export function selectRoute(root: string, inputs: Inputs, fetchBase = false): Route {
  const { event, base } = inputs;
  if (
    !["pull_request", "push"].includes(event ?? "") ||
    !base ||
    !/^[a-f0-9]{40}$/.test(base) ||
    /^0+$/.test(base)
  )
    return full;
  try {
    try {
      git(root, ["cat-file", "-e", `${base}^{commit}`]);
    } catch {
      if (!fetchBase) return full;
      git(root, ["fetch", "--no-tags", "--depth=1", "origin", base]);
      git(root, ["cat-file", "-e", `${base}^{commit}`]);
    }
    // No rename detection: both sides are evaluated, and deletions conservatively go full.
    const raw = git(root, [
      "diff",
      "--raw",
      "-z",
      "--no-renames",
      "--no-ext-diff",
      "--no-textconv",
      base,
      "HEAD",
      "--",
    ]);
    if (!raw || !raw.endsWith("\0")) return full;
    const records = raw.slice(0, -1).split("\0");
    if (records.length % 2 !== 0) return full;
    const files: string[] = [];
    for (let i = 0; i < records.length; i += 2) {
      const header = /^:(\d{6}) (\d{6}) [a-f0-9]+ [a-f0-9]+ ([AM])$/.exec(records[i]!);
      const path = records[i + 1]!;
      if (
        !header ||
        header[2] !== "100644" ||
        (header[3] === "A" ? header[1] !== "000000" : header[1] !== "100644") ||
        !isDocumentation(path)
      )
        return full;
      files.push(path);
    }
    return files.length ? { route: "light", files } : full;
  } catch {
    return full;
  }
}

function prose(text: string): string {
  let fence: { marker: string; length: number } | undefined;
  const lines: string[] = [];
  for (const original of text.split(/\r?\n/)) {
    let line = original;
    while (/^ {0,3}>[ \t]?/.test(line)) line = line.replace(/^ {0,3}>[ \t]?/, "");
    const opening = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
    if (fence) {
      const closing = /^ {0,3}(`+|~+)\s*$/.exec(line);
      if (closing && closing[1]![0] === fence.marker && closing[1]!.length >= fence.length)
        fence = undefined;
    } else if (opening) {
      fence = { marker: opening[1]![0]!, length: opening[1]!.length };
    } else if (!/^( {4}|\t)/.test(line)) {
      lines.push(line);
    }
  }
  if (fence) throw new Error("Unclosed code fence");
  return lines.join("\n").replace(/(`+)[\s\S]*?\1/g, "");
}

function linkTargets(body: string): string[] {
  const targets: string[] = [];
  for (let i = 0; i < body.length; i++) {
    if (body[i] === "\\") {
      i++;
      continue;
    }
    if (body[i] !== "[") continue;
    let labelDepth = 1;
    let close = i + 1;
    for (; close < body.length; close++) {
      if (body[close] === "\\") {
        close++;
      } else if (body[close] === "[") {
        labelDepth++;
      } else if (body[close] === "]" && --labelDepth === 0) {
        break;
      }
    }
    if (labelDepth || body[close + 1] !== "(") continue;
    let start = close + 2;
    while (start < body.length && /\s/.test(body[start]!)) start++;
    if (body[start] === "<") {
      const end = body.indexOf(">", start + 1);
      if (end !== -1) {
        targets.push(body.slice(start + 1, end));
        i = end;
      }
      continue;
    }
    let depth = 0;
    let end = start;
    for (; end < body.length; end++) {
      const char = body[end];
      if (char === "\\") {
        end++;
      } else if (char === "(") {
        depth++;
      } else if (char === ")") {
        if (!depth) break;
        depth--;
      } else if (/\s/.test(char!)) {
        break;
      }
    }
    if (!depth) targets.push(body.slice(start, end));
    i = end;
  }
  for (const match of body.matchAll(/^ {0,3}\[[^\]\n]+\]:\s*(<[^>\n]*>|\S+)/gm))
    targets.push(match[1]!.replace(/^<|>$/g, ""));
  return targets;
}

export function validateDocument(root: string, file: string, text: string): void {
  if (/^\.agents\/skills\/[^/]+\/SKILL\.md$/.test(file)) {
    const match = /^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/.exec(text);
    if (!match) throw new Error("Missing skill YAML frontmatter");
    const yaml = Bun.YAML.parse(match[1]!);
    if (
      !yaml ||
      typeof yaml !== "object" ||
      Array.isArray(yaml) ||
      !("name" in yaml) ||
      typeof yaml.name !== "string" ||
      !yaml.name.trim() ||
      !("description" in yaml) ||
      typeof yaml.description !== "string" ||
      !yaml.description.trim()
    )
      throw new Error("Skill YAML requires name and description");
    text = text.slice(match[0].length);
  }
  const body = prose(text);
  for (const raw of linkTargets(body)) {
    const target = raw.replace(/\\([\\()[\]<> ])/g, "$1");
    if (/^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i.test(target)) continue;
    const path = decodeURIComponent(target.split(/[?#]/, 1)[0]!);
    if (!path) continue;
    const destination = path.startsWith("/")
      ? resolve(root, `.${path}`)
      : resolve(dirname(resolve(root, file)), path);
    if (!existsSync(destination)) throw new Error(`Missing local link: ${target}`);
    const within = relative(realpathSync(root), realpathSync(destination));
    if (within === ".." || within.startsWith("../") || isAbsolute(within))
      throw new Error(`Local link escapes checkout: ${target}`);
  }
}

function inputsFromEnv(): Inputs {
  return { event: process.env.CI_EVENT, base: process.env.CI_BASE };
}

if (import.meta.main) {
  const command = process.argv[2];
  const root = process.cwd();
  if (command === "select") {
    const selection = selectRoute(root, inputsFromEnv(), true);
    console.log(selection.route);
    if (process.env.GITHUB_OUTPUT)
      appendFileSync(process.env.GITHUB_OUTPUT, `route=${selection.route}\n`);
  } else if (command === "check") {
    try {
      const selection = selectRoute(root, inputsFromEnv());
      if (selection.route !== "light")
        throw new Error("Docs check requires a complete docs-only diff");
      for (const file of selection.files)
        validateDocument(root, file, readFileSync(resolve(root, file), "utf8"));
      const format = spawnSync(
        resolve(root, "node_modules/.bin/oxfmt"),
        ["--check", ...selection.files],
        {
          cwd: root,
          stdio: "inherit",
        },
      );
      if (format.error || format.status !== 0) throw new Error("Documentation formatting failed");
    } catch (error) {
      console.error(error);
      process.exitCode = 1;
    }
  } else {
    console.error("Usage: bun scripts/ci-docs.ts select|check");
    process.exitCode = 1;
  }
}
