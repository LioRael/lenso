import { readFile as nodeReadFile } from "node:fs/promises";
import { isAbsolute, relative, resolve, sep } from "node:path";
import { ConfigSourceError } from "./config";
import type { ConfigReadContext, ConfigSource } from "./config-types";
import type { PluginSource } from "./plugin";

const dangerousKeys = new Set(["__proto__", "prototype", "constructor"]);

export interface JsonFileSourceOptions {
  readonly id: string;
  readonly root: string;
  readonly path: string;
  readonly select?: readonly string[];
  readonly optional?: boolean;
  readonly location?: PluginSource;
  readonly readFile?: (absolutePath: string, context: ConfigReadContext) => Promise<string>;
}

export function jsonFileSource(options: JsonFileSourceOptions): ConfigSource {
  if (!isAbsolute(options.root)) throw new ConfigSourceError("config-file-invalid");
  if (isAbsolute(options.path)) throw new ConfigSourceError("config-file-invalid");
  if (options.select?.some((key) => dangerousKeys.has(key))) {
    throw new ConfigSourceError("config-file-invalid");
  }
  const absolutePath = resolve(options.root, options.path);
  const pathFromRoot = relative(resolve(options.root), absolutePath);
  if (pathFromRoot === ".." || pathFromRoot.startsWith(`..${sep}`) || isAbsolute(pathFromRoot)) {
    throw new ConfigSourceError("config-file-invalid");
  }
  const select = options.select ? [...options.select] : undefined;
  const optional = options.optional ?? false;
  const readFile =
    options.readFile ??
    ((path: string, context: ConfigReadContext) =>
      nodeReadFile(path, { encoding: "utf8", signal: context.signal }));
  return {
    descriptor: { id: options.id, kind: "file", location: options.location },
    async read(context) {
      if (context.signal?.aborted) throw new ConfigSourceError("config-cancelled");
      let text: string;
      try {
        text = await readFile(absolutePath, context);
      } catch (error) {
        if (context.signal?.aborted) throw new ConfigSourceError("config-cancelled");
        if (optional && isErrno(error, "ENOENT")) return { values: {} };
        if (isErrno(error, "ENOENT")) throw new ConfigSourceError("config-file-missing");
        throw new ConfigSourceError("config-file-invalid");
      }
      if (context.signal?.aborted) throw new ConfigSourceError("config-cancelled");
      let parsed: unknown;
      try {
        parsed = JSON.parse(text);
      } catch {
        throw new ConfigSourceError("config-file-invalid");
      }
      if (!isPlainObject(parsed)) throw new ConfigSourceError("config-file-invalid");
      for (const key of select ?? []) {
        if (!isPlainObject(parsed) || !Object.hasOwn(parsed, key))
          throw new ConfigSourceError("config-file-invalid");
        parsed = parsed[key];
      }
      if (!isPlainObject(parsed)) throw new ConfigSourceError("config-file-invalid");
      return { values: parsed };
    },
  };
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function isErrno(error: unknown, code: string): boolean {
  return typeof error === "object" && error !== null && "code" in error && error.code === code;
}
