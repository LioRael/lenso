import type { StandardSchemaV1 } from "@standard-schema/spec";
import { ConfigError, preflightConfigs, resolveConfig, valuesSource } from "./config";
import type { ConfigContract, ConfigDiagnostic } from "./config-types";
import type { Plugin } from "./plugin";

export type InitializationAction = "status" | "apply" | "activate";
export type InitializationBlocker =
  | {
      readonly code:
        | "database-unavailable"
        | "external-configuration-required"
        | "activation-failed";
    }
  | { readonly code: "configuration-invalid"; readonly diagnostics: readonly ConfigDiagnostic[] };
/** Persist alongside the first owner/configuration, in the SAME transaction. No secrets. */
export interface InitializationRecord {
  readonly revision: string;
  readonly requestId: string;
  readonly activated: boolean;
}
export interface InitializationStatus {
  readonly schemaVersion: 1;
  readonly state: "uninitialized" | "blocked" | "awaiting_activation" | "ready";
  readonly revision: string | null;
  readonly blockers: readonly InitializationBlocker[];
  readonly nextAction: "apply" | "external_configuration" | "activate" | "none";
}
export type InitializationCode =
  | "initialization-required"
  | "initialization-not-ready"
  | "initialization-unauthorized"
  | "initialization-invalid-input"
  | "initialization-closed"
  | "initialization-conflict"
  | "initialization-unavailable"
  | "initialization-outcome-unknown";
const messages: Record<InitializationCode, string> = {
  "initialization-required": "Application is not initialized. Use lenso setup status.",
  "initialization-not-ready":
    "Initialization is not ready. Use lenso setup status before continuing.",
  "initialization-unauthorized": "A current trusted initialization authorization is required.",
  "initialization-invalid-input": "Initialization input does not match its declared contract.",
  "initialization-closed": "First-owner initialization is closed for this application.",
  "initialization-conflict": "Initialization request conflicts with the committed request.",
  "initialization-unavailable":
    "Cannot inspect initialization. Check the selected host and database.",
  "initialization-outcome-unknown":
    "Initialization write outcome is unknown. Query setup status; do not replay blindly.",
};
export class InitializationError extends Error {
  constructor(
    readonly code: InitializationCode,
    options?: ErrorOptions,
  ) {
    super(messages[code], options);
    this.name = "InitializationError";
  }
}
export interface Initialization {
  /** Internal startup fence, not an HTTP authorization check. Never initializes. */
  assertReady(): Promise<void>;
  status(evidence: unknown): Promise<InitializationStatus>;
  apply(
    request: { readonly requestId: string; readonly input: unknown },
    evidence: unknown,
  ): Promise<InitializationStatus>;
  activate(evidence: unknown): Promise<InitializationStatus>;
}
export interface InitializationOptions<S extends StandardSchemaV1> {
  readonly input: ConfigContract<S>;
  /** Verify actual host evidence. A JSON actor/owner flag is not authorization. */
  readonly authorize: (
    action: InitializationAction,
    evidence: unknown,
  ) => boolean | Promise<boolean>;
  /** Read only. No schema creation or secret provisioning. */
  readonly read: () => Promise<InitializationRecord | null>;
  /** Host capability/DB checks BEFORE writing. Workers can require an external secret writer. */
  readonly beforeCommit?: () => Promise<readonly InitializationBlocker[]>;
  /** Atomically reserve the first owner + pending marker + config references, with durable CAS/idempotency.
   * Cross-provider credentials require existing durable staging/receipts before completion. */
  readonly commit: (request: {
    readonly requestId: string;
    readonly input: StandardSchemaV1.InferOutput<S>;
  }) => Promise<InitializationRecord>;
  /** Validate persisted config against the ACTUAL selected environment, no values in output. */
  readonly check: (record: InitializationRecord) => Promise<readonly InitializationBlocker[]>;
  /** Optional host-owned, idempotent activation. No payment/model/business call. */
  readonly activate?: (record: InitializationRecord) => Promise<void>;
  /** CAS the same revision after successful checks/activation. Must not recreate an owner. */
  readonly complete: (revision: string) => Promise<InitializationRecord>;
}

/** Uses existing configuration resolution, not a second schema/secret-provider system. */
export async function initializationConfigBlockers(
  plugins: readonly Plugin<unknown>[],
): Promise<readonly InitializationBlocker[]> {
  try {
    await preflightConfigs(plugins);
    return [];
  } catch (error) {
    if (!(error instanceof ConfigError)) throw error;
    return [{ code: "configuration-invalid", diagnostics: error.diagnostics }];
  }
}

const definitions = new WeakSet<object>();
export function isInitialization(value: unknown): value is Initialization {
  return typeof value === "object" && value !== null && definitions.has(value);
}
export function defineInitialization<S extends StandardSchemaV1>(
  options: InitializationOptions<S>,
): Initialization {
  async function authorize(action: InitializationAction, evidence: unknown) {
    let allowed = false;
    try {
      allowed = (await options.authorize(action, evidence)) === true;
    } catch {
      /* fail closed */
    }
    if (!allowed) throw new InitializationError("initialization-unauthorized");
  }
  async function read() {
    try {
      return record(await options.read());
    } catch (cause) {
      if (cause instanceof InitializationError && cause.code === "initialization-closed")
        throw cause;
      throw new InitializationError("initialization-unavailable", { cause });
    }
  }
  function record(value: InitializationRecord | null): InitializationRecord | null {
    if (value === null) return null;
    if (
      typeof value.revision !== "string" ||
      typeof value.requestId !== "string" ||
      !/^[A-Za-z0-9_-]{1,128}$/.test(value.revision) ||
      !/^[A-Za-z0-9_-]{8,128}$/.test(value.requestId) ||
      typeof value.activated !== "boolean"
    )
      throw new InitializationError("initialization-unavailable");
    return { revision: value.revision, requestId: value.requestId, activated: value.activated };
  }
  async function inspect(current: InitializationRecord | null): Promise<InitializationStatus> {
    let blockers: readonly InitializationBlocker[];
    try {
      blockers = safeBlockers(
        current ? await options.check(current) : ((await options.beforeCommit?.()) ?? []),
      );
    } catch (cause) {
      throw new InitializationError("initialization-unavailable", { cause });
    }
    return {
      schemaVersion: 1,
      state: blockers.length
        ? "blocked"
        : current
          ? current.activated
            ? "ready"
            : "awaiting_activation"
          : "uninitialized",
      revision: current?.revision ?? null,
      blockers,
      nextAction: blockers.some((item) => item.code !== "activation-failed")
        ? "external_configuration"
        : current
          ? current.activated && !blockers.length
            ? "none"
            : "activate"
          : "apply",
    };
  }
  async function activate(
    current: InitializationRecord,
    evidence: unknown,
  ): Promise<InitializationStatus> {
    await authorize("activate", evidence);
    const status = await inspect(current);
    if (
      status.blockers.some((item) => item.code !== "activation-failed") ||
      (current.activated && !status.blockers.length)
    )
      return status;
    try {
      await options.activate?.(current);
    } catch {
      return {
        ...status,
        state: "awaiting_activation",
        blockers: [{ code: "activation-failed" }],
        nextAction: "activate",
      };
    }
    await authorize("activate", evidence);
    let committed;
    try {
      committed = record(await options.complete(current.revision));
    } catch (cause) {
      if (cause instanceof InitializationError && cause.code === "initialization-unauthorized")
        throw cause;
      throw new InitializationError("initialization-outcome-unknown", { cause });
    }
    if (
      !committed ||
      committed.revision !== current.revision ||
      committed.requestId !== current.requestId ||
      !committed.activated
    )
      throw new InitializationError("initialization-outcome-unknown");
    return inspect(committed);
  }
  const definition: Initialization = {
    async assertReady() {
      const current = await read();
      if (!current) throw new InitializationError("initialization-required");
      if (!current.activated || (await inspect(current)).state !== "ready")
        throw new InitializationError("initialization-not-ready");
    },
    async status(evidence) {
      await authorize("status", evidence);
      return inspect(await read());
    },
    async apply(request, evidence) {
      await authorize("apply", evidence);
      if (
        !request ||
        typeof request.requestId !== "string" ||
        !/^[A-Za-z0-9_-]{8,128}$/.test(request.requestId)
      )
        throw new InitializationError("initialization-invalid-input");
      let input: StandardSchemaV1.InferOutput<S>;
      try {
        input = (
          await resolveConfig("initialization", {
            contract: options.input,
            sources: [valuesSource(request.input as Record<string, unknown>)],
          })
        ).value;
      } catch (cause) {
        throw new InitializationError("initialization-invalid-input", { cause });
      }
      const current = await read();
      if (current && current.requestId !== request.requestId)
        throw new InitializationError("initialization-closed");
      const blockers = safeBlockers((await options.beforeCommit?.()) ?? []);
      if (blockers.length)
        return {
          schemaVersion: 1,
          state: "blocked",
          revision: current?.revision ?? null,
          blockers,
          nextAction: blockers.some((item) => item.code !== "activation-failed")
            ? "external_configuration"
            : "apply",
        };
      await authorize("apply", evidence);
      let committed;
      try {
        committed = record(await options.commit({ requestId: request.requestId, input }));
      } catch (cause) {
        if (
          cause instanceof InitializationError &&
          [
            "initialization-closed",
            "initialization-conflict",
            "initialization-unauthorized",
          ].includes(cause.code)
        )
          throw cause;
        throw new InitializationError("initialization-outcome-unknown", { cause });
      }
      if (!committed || committed.requestId !== request.requestId)
        throw new InitializationError("initialization-outcome-unknown");
      return activate(committed, evidence);
    },
    async activate(evidence) {
      await authorize("activate", evidence);
      const current = await read();
      if (!current) throw new InitializationError("initialization-required");
      return activate(current, evidence);
    },
  };
  definitions.add(definition);
  return Object.freeze(definition);
}

/** Closed DTO projection: callback extras/messages/values are never serialized. */
function safeBlockers(items: readonly InitializationBlocker[]): readonly InitializationBlocker[] {
  if (!Array.isArray(items) || items.length > 32)
    throw new InitializationError("initialization-unavailable");
  return items.map((item) => {
    if (
      ["database-unavailable", "external-configuration-required", "activation-failed"].includes(
        item.code,
      )
    )
      return { code: item.code } as InitializationBlocker;
    if (item.code === "configuration-invalid") {
      const diagnostics = new ConfigError(item.diagnostics.slice(0, 32)).diagnostics;
      return {
        code: item.code,
        diagnostics: diagnostics.map(({ code, pluginId, path, sourceId, source }) => ({
          code,
          pluginId,
          ...(path ? { path: [...path] } : {}),
          ...(sourceId ? { sourceId } : {}),
          ...(source
            ? { source: { file: source.file, ...(source.export ? { export: source.export } : {}) } }
            : {}),
        })),
      };
    }
    throw new InitializationError("initialization-unavailable");
  });
}
