import { ConfigSourceError } from "./config";
import type { ConfigPath, ConfigSource } from "./config-types";
import type { PluginSource } from "./plugin";

export interface EnvSourceOptions {
  readonly id: string;
  readonly read: (name: string) => string | undefined | Promise<string | undefined>;
  readonly bindings: Readonly<
    Record<
      string,
      {
        readonly name: string;
        readonly type?: "string" | "number" | "boolean";
        readonly sensitive?: boolean;
        readonly empty?: "preserve" | "omit" | "error";
      }
    >
  >;
  readonly location?: PluginSource;
}

export function envSource(options: EnvSourceOptions): ConfigSource {
  const bindings = Object.entries(options.bindings).map(
    ([field, binding]) => [field, { ...binding }] as const,
  );
  const read = options.read;
  const fields = bindings.map(([field, binding]) => ({
    path: [field] as ConfigPath,
    env: binding.name,
    ...(binding.sensitive ? { sensitive: true } : {}),
  }));
  return {
    descriptor: { id: options.id, kind: "env", location: options.location, fields },
    async read(context) {
      const values: Record<string, unknown> = Object.create(null);
      for (const [field, binding] of bindings) {
        if (["__proto__", "constructor", "prototype"].includes(field))
          throw new ConfigSourceError("config-env-invalid", [field]);
        if (context.signal?.aborted) throw new ConfigSourceError("config-cancelled");
        let raw: string | undefined;
        try {
          raw = await read(binding.name);
        } catch {
          throw new ConfigSourceError("config-source-failed", [field]);
        }
        if (context.signal?.aborted) throw new ConfigSourceError("config-cancelled");
        if (raw === undefined) continue;
        if (typeof raw !== "string") throw new ConfigSourceError("config-env-invalid", [field]);
        const type = binding.type ?? "string";
        const empty = binding.empty ?? (type === "string" ? "preserve" : "error");
        if (raw === "") {
          if (empty === "omit") continue;
          if (empty === "error" || (empty === "preserve" && type !== "string")) {
            throw new ConfigSourceError("config-env-invalid", [field]);
          }
        }
        if (type === "string") values[field] = raw;
        else if (type === "boolean") {
          if (raw !== "true" && raw !== "false")
            throw new ConfigSourceError("config-env-invalid", [field]);
          values[field] = raw === "true";
        } else {
          if (!/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(raw)) {
            throw new ConfigSourceError("config-env-invalid", [field]);
          }
          const number = Number(raw);
          if (!Number.isFinite(number)) throw new ConfigSourceError("config-env-invalid", [field]);
          values[field] = number;
        }
      }
      return { values };
    },
  };
}
