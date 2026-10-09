import type { Plugin } from "@lenso/core";
import {
  describeOperation,
  redactOperationDescription,
  validateOperations,
  type Operation,
} from "@lenso/engine/operations";
import { EngineError, environmentSecrets, redact, stableJson } from "@lenso/engine/diagnostics";

export interface ManageView {
  readonly key: string;
  readonly title?: string;
  readonly group?: string;
  readonly order?: number;
  readonly columns?: readonly string[];
  readonly detail?: string;
  readonly action?: string;
}

export interface Manage<
  P extends Plugin<unknown> = Plugin<unknown>,
  O extends readonly Operation[] = readonly Operation[],
> {
  readonly plugin: P;
  readonly operations: O;
  readonly views: readonly ManageView[];
  readonly extensions: Readonly<Record<string, unknown>>;
}

function manageFailure(code: string, message: string, phase = "discovery"): EngineError {
  return new EngineError({ code, phase, message });
}

function plainSnapshot<T>(value: T): T {
  const seen = new WeakSet<object>();
  function check(item: unknown): void {
    if (item === null || ["string", "boolean"].includes(typeof item)) return;
    if (typeof item === "number" && Number.isFinite(item)) return;
    if (!item || typeof item !== "object" || seen.has(item))
      throw manageFailure("invalid-manage", "Manage metadata must be plain acyclic JSON.");
    if (!Array.isArray(item) && ![Object.prototype, null].includes(Object.getPrototypeOf(item)))
      throw manageFailure("invalid-manage", "Manage metadata must contain plain JSON objects.");
    seen.add(item);
    for (const key of Reflect.ownKeys(item)) {
      if (Array.isArray(item) && key === "length") continue;
      const property = Object.getOwnPropertyDescriptor(item, key)!;
      if (typeof key !== "string" || !property.enumerable || !("value" in property))
        throw manageFailure(
          "invalid-manage",
          "Manage metadata cannot contain accessors or symbols.",
        );
      check(property.value);
    }
    seen.delete(item);
  }
  check(value);
  const copy = JSON.parse(stableJson(value)) as T;
  function freeze(item: unknown): void {
    if (!item || typeof item !== "object") return;
    Object.values(item).forEach(freeze);
    Object.freeze(item);
  }
  freeze(copy);
  return copy;
}

export function defineManage<
  P extends Plugin<unknown>,
  const O extends readonly Operation[],
>(options: {
  plugin: P;
  operations: O;
  views?: readonly ManageView[];
  extensions?: Readonly<Record<string, unknown>>;
}): Manage<P, O> {
  validateOperations([options.plugin], options.operations);
  const operations = Object.freeze([...options.operations]) as unknown as O;
  const views = plainSnapshot(options.views ?? []);
  const extensions = plainSnapshot(options.extensions ?? {});
  if (!Array.isArray(views) || !extensions || Array.isArray(extensions))
    throw manageFailure(
      "invalid-manage",
      "Views must be an array and extensions a namespaced object.",
    );
  const methods = new Set(operations.map((operation) => operation.method));
  const keys = new Set<string>();
  const allowed = new Set(["key", "title", "group", "order", "columns", "detail", "action"]);
  for (const view of views) {
    if (
      !view ||
      typeof view !== "object" ||
      Object.keys(view).some((key) => !allowed.has(key)) ||
      typeof view.key !== "string" ||
      !/^[a-zA-Z0-9][a-zA-Z0-9._/-]*$/.test(view.key) ||
      keys.has(view.key) ||
      ["title", "group", "detail", "action"].some(
        (key) => Reflect.get(view, key) !== undefined && typeof Reflect.get(view, key) !== "string",
      ) ||
      (view.order !== undefined && !Number.isFinite(view.order)) ||
      (view.columns !== undefined &&
        (!Array.isArray(view.columns) ||
          view.columns.some((column: unknown) => typeof column !== "string" || !column))) ||
      (view.detail !== undefined && !methods.has(view.detail)) ||
      (view.action !== undefined && !methods.has(view.action))
    )
      throw manageFailure(
        "invalid-manage-view",
        "Views need unique keys, minimal hints and declared operation references.",
      );
    keys.add(view.key);
  }
  if (
    Object.keys(extensions).some(
      (key) => !/^[a-zA-Z0-9][a-zA-Z0-9._-]*[:/][a-zA-Z0-9._/-]+$/.test(key),
    )
  )
    throw manageFailure("invalid-manage-extension", "Extension keys must be namespaced.");
  return Object.freeze({ plugin: options.plugin, operations, views, extensions });
}

export function selectManageOperations<M extends Manage>(
  manage: M,
  methods: readonly M["operations"][number]["method"][],
): readonly M["operations"][number][] {
  validateOperations([manage.plugin], manage.operations);
  const selected = new Set<string>();
  return Object.freeze(
    methods.map((method) => {
      if (selected.has(method))
        throw manageFailure(
          "duplicate-operation",
          "Manage selections must not contain duplicate methods.",
        );
      selected.add(method);
      const operation = manage.operations.find((item) => item.method === method);
      if (!operation)
        throw manageFailure(
          "unknown-operation",
          "Manage selection must name a declared operation.",
        );
      return operation;
    }),
  );
}

export function describeManage(manage: Manage, configPath = "lenso.config.ts") {
  const snapshot = defineManage(manage);
  return plainSnapshot({
    schemaVersion: 1 as const,
    pluginId: redact(snapshot.plugin.id, environmentSecrets()) as string,
    operations: snapshot.operations.map((operation) =>
      redactOperationDescription(describeOperation(operation, configPath)),
    ),
    views: redact(snapshot.views, environmentSecrets()) as readonly ManageView[],
    extensions: redact(snapshot.extensions, environmentSecrets()) as Readonly<
      Record<string, unknown>
    >,
  });
}

export { createManageAdapter, createManageSelection, bindManageOperation } from "./adapter";
export type {
  ManageAdapter,
  ManageAdapterOptions,
  ManageRequestOptions,
  ManageSelection,
  ManageSelectionOptions,
  ManageInvocationBinding,
  ManageCatalogEntry,
} from "./adapter";
export { createAgentTools } from "./agent";
export type { AgentTool } from "./agent";
