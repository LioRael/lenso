import type { StandardSchemaV1 } from "@standard-schema/spec";
import type { PluginSource } from "./plugin";

export type ConfigPath = readonly (string | number)[];

export interface ConfigField {
  readonly path: ConfigPath;
  readonly description?: string;
  readonly sensitive?: boolean;
}

export interface ConfigContract<S extends StandardSchemaV1 = StandardSchemaV1> {
  readonly schema: S;
  readonly description?: string;
  readonly fields?: readonly ConfigField[];
  /** Trusted, explicitly selected converter; never inferred from validator internals. */
  readonly jsonSchema?: () => Record<string, unknown>;
}

export interface ConfigSourceDescription {
  readonly id: string;
  readonly kind: string;
  readonly location?: PluginSource;
  readonly fields?: readonly {
    readonly path: ConfigPath;
    readonly env?: string;
    readonly sensitive?: boolean;
  }[];
}

export interface ConfigReadContext {
  readonly signal?: AbortSignal;
}

export interface ConfigSourceResult {
  readonly values: Readonly<Record<string, unknown>>;
  readonly revision?: unknown;
}

/** Capabilities such as env access or file reading are granted to each adapter, not the resolver. */
export interface ConfigSource {
  readonly descriptor: ConfigSourceDescription;
  read(context: ConfigReadContext): Promise<ConfigSourceResult>;
}

export interface ConfigBinding<S extends StandardSchemaV1 = StandardSchemaV1> {
  readonly contract: ConfigContract<S>;
  readonly sources: readonly ConfigSource[];
}

export interface ConfigDiagnostic {
  readonly code:
    | "config-source-failed"
    | "config-invalid-data"
    | "config-invalid"
    | "config-cancelled"
    | "config-env-invalid"
    | "config-file-missing"
    | "config-file-invalid";
  readonly pluginId: string;
  readonly path?: ConfigPath;
  readonly sourceId?: string;
  readonly source?: PluginSource;
}

export interface ConfigProvenance {
  /** Raw input fields only; schema-derived output fields have no inferred provenance. */
  readonly path: ConfigPath;
  readonly sourceIds: readonly string[];
  readonly sensitive: boolean;
}

export interface ConfigSnapshot<Output = unknown> {
  readonly value: Output;
  readonly provenance: readonly ConfigProvenance[];
  readonly revisions: readonly { readonly sourceId: string; readonly revision: unknown }[];
}
