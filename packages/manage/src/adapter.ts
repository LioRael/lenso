import { validatePluginSelection, type Plugin } from "@lenso/core";
import {
  describeOperation,
  boundedJson,
  invokeValidatedOperation,
  operationError,
  redactOperationDescription,
  validateOperationInput,
  validateOperations,
  type Operation,
  type OperationBoundOptions,
  type OperationRuntime,
} from "@lenso/engine/operations";
import { diagnostic, EngineError } from "@lenso/engine/diagnostics";

export interface ManageInvocationBinding<C = unknown> {
  readonly context?: C;
  readonly signal?: AbortSignal;
  readonly confirm?: () => boolean | Promise<boolean>;
  readonly approve?: () => boolean | Promise<boolean>;
}

export interface ManageSelectionOptions<O extends Operation = Operation> {
  readonly running: OperationRuntime;
  readonly plugins: readonly Plugin<unknown>[];
  readonly operations: readonly O[];
}

export interface ManageRequestOptions<O extends Operation = Operation> {
  readonly binding: (
    operation: O,
    validatedInput: unknown,
  ) => OperationBoundOptions<NoInfer<O>> | Promise<OperationBoundOptions<NoInfer<O>>>;
  readonly canList: (operation: O) => boolean | Promise<boolean>;
  readonly maxOutputBytes?: number;
  /** Trusted host opt-in for independent catalog policies; dispatch stays serial. */
  readonly catalogConcurrency?: number;
}

export interface ManageSelection<O extends Operation = Operation> {
  createAdapter(options: ManageRequestOptions<O>): ManageAdapter;
  /** Revokes future dispatch and releases this selection's references, without stopping borrowed services. */
  close(): void;
}

export type ManageAdapterOptions<O extends Operation = Operation> = ManageRequestOptions<O> &
  (ManageSelectionOptions<O> | { readonly selection: ManageSelection<O> });

export type ManageCatalogEntry = ReturnType<typeof describeOperation> & {
  readonly schemaVersion: 1;
  readonly key: string;
};

export interface ManageAdapter {
  catalog(): Promise<readonly ManageCatalogEntry[]>;
  invoke(pluginId: string, method: string, input: unknown): Promise<unknown>;
  invokeEntry(key: string, input: unknown): Promise<unknown>;
}

function refuse(code: string, message: string): never {
  throw new EngineError({ code, phase: "invoke", message });
}

export function bindManageOperation<C>(
  operation: Operation<C>,
  binding: ManageInvocationBinding<NoInfer<C>> & { readonly context: NoInfer<C> },
) {
  return { operation, binding };
}

export function createManageAdapter<O extends Operation>(
  options: ManageAdapterOptions<O>,
): ManageAdapter {
  const selection = "selection" in options ? options.selection : createManageSelection(options);
  return selection.createAdapter(options);
}

interface SelectionEntry<O extends Operation> {
  readonly key: string;
  readonly pluginId: string;
  readonly operation: O;
}

function prepareSelection<O extends Operation>(options: ManageSelectionOptions<O>) {
  const plugins = Object.freeze([...options.plugins]);
  validatePluginSelection(plugins);
  if (!Array.isArray(options.operations)) validateOperations(plugins, options.operations);
  const operations = Object.freeze(
    options.operations.map((operation) => {
      const snapshot = { ...operation };
      return Object.freeze({
        ...snapshot,
        ...(snapshot.errors === undefined
          ? {}
          : { errors: Object.freeze(snapshot.errors.map((error) => Object.freeze({ ...error }))) }),
        ...(snapshot.source === undefined ? {} : { source: Object.freeze({ ...snapshot.source }) }),
      });
    }),
  );
  validateOperations(plugins, operations);
  for (const plugin of plugins) {
    try {
      options.running.get(plugin);
    } catch (cause) {
      throw new EngineError(
        {
          code: "invalid-manage",
          phase: "discovery",
          message: "Selected plugin must be the exact running plugin instance.",
          instanceId: options.running.instanceId,
          pluginId: plugin.id,
        },
        { cause },
      );
    }
  }
  const nonce = crypto.randomUUID().replaceAll("-", "");
  const byPlugin = new Map<string, Map<string, SelectionEntry<O>>>(
    plugins.map((plugin) => [plugin.id, new Map()]),
  );
  const entries: SelectionEntry<O>[] = operations.map((operation, index) => {
    const entry = Object.freeze({
      key: `operation_${nonce}_${index}`,
      pluginId: operation.plugin.id,
      operation,
    });
    byPlugin.get(entry.pluginId)!.set(operation.method, entry);
    return entry;
  });
  return {
    running: options.running,
    entries,
    byPlugin,
    byKey: new Map<string, SelectionEntry<O>>(entries.map((entry) => [entry.key, entry])),
  };
}

export function createManageSelection<O extends Operation>(
  options: ManageSelectionOptions<O>,
): ManageSelection<O> {
  return selectionFromState(prepareSelection(options));
}

function selectionFromState<O extends Operation>(
  state: ReturnType<typeof prepareSelection<O>> | undefined,
): ManageSelection<O> {
  function current() {
    if (!state) refuse("closed", "Manage selection is closed.");
    return state;
  }
  function target(entry: SelectionEntry<O>) {
    const active = current();
    const operation = entry.operation;
    if (active.byKey.get(entry.key) !== entry || operation.plugin.id !== entry.pluginId)
      refuse("unavailable-operation", "Manage target is no longer valid.");
    validatePluginSelection([operation.plugin]);
    validateOperations([operation.plugin], [operation]);
    active.running.get(operation.plugin);
    return active.running;
  }
  return Object.freeze({
    close() {
      if (!state) return;
      state.byKey.clear();
      state.byPlugin.clear();
      state.entries.length = 0;
      state = undefined;
    },
    createAdapter({
      binding,
      canList,
      maxOutputBytes,
      catalogConcurrency = 1,
    }: ManageRequestOptions<O>) {
      current();
      if (typeof binding !== "function" || typeof canList !== "function")
        refuse("invalid-manage", "Manage requires invocation binding and caller catalog policy.");
      if (
        maxOutputBytes !== undefined &&
        (!Number.isSafeInteger(maxOutputBytes) || maxOutputBytes < 1)
      )
        refuse("invalid-manage", "Output byte limit must be a positive safe integer.");
      if (
        !Number.isSafeInteger(catalogConcurrency) ||
        catalogConcurrency < 1 ||
        catalogConcurrency > 8
      )
        refuse("invalid-manage", "Catalog concurrency must be an integer from 1 to 8.");
      async function visible(entry: SelectionEntry<O>) {
        target(entry);
        const allowed = await canList(entry.operation);
        target(entry);
        return allowed === true;
      }
      async function invoke(entry: SelectionEntry<O>, input: unknown) {
        const operation = entry.operation;
        if (!(await visible(entry)))
          refuse("forbidden-operation", "Operation is not available to this caller.");
        const validated = await validateOperationInput(operation, input);
        target(entry);
        let invocation;
        try {
          invocation = await binding(operation, validated);
        } catch (error) {
          throw operationError(operation, error);
        }
        if (!invocation || typeof invocation !== "object")
          refuse("invalid-manage-binding", "Invocation binding must return a trusted binding.");
        const running = target(entry);
        const guarded: OperationRuntime = {
          instanceId: running.instanceId,
          ...(running.logger === undefined ? {} : { logger: running.logger }),
          get: <T>(plugin: Plugin<T>) => target(entry).get(plugin),
        };
        return await invokeValidatedOperation<Operation>(guarded, operation, validated, {
          context: invocation.context,
          ...(invocation.signal === undefined ? {} : { signal: invocation.signal }),
          ...(invocation.confirm === undefined ? {} : { confirm: invocation.confirm }),
          ...(invocation.approve === undefined ? {} : { approve: invocation.approve }),
          ...(maxOutputBytes === undefined ? {} : { maxOutputBytes }),
          beforeExecute: async () => {
            if (!(await visible(entry)))
              refuse("forbidden-operation", "Operation is not available to this caller.");
          },
        });
      }
      async function dispatch(resolve: () => SelectionEntry<O>, input: unknown) {
        const instanceId = current().running.instanceId;
        let entry: SelectionEntry<O> | undefined;
        try {
          entry = resolve();
          return await invoke(entry, input);
        } catch (error) {
          if (error instanceof EngineError) throw error;
          throw new EngineError(
            diagnostic(error, {
              instanceId,
              ...(entry
                ? {
                    pluginId: entry.pluginId,
                    operation: `${entry.pluginId}.${entry.operation.method}`,
                  }
                : {}),
              phase: "invoke",
            }),
            { cause: error },
          );
        }
      }
      return Object.freeze({
        async catalog() {
          const instanceId = current().running.instanceId;
          try {
            const entries = [...current().entries];
            const projected: (ManageCatalogEntry | undefined)[] = Array.from(
              { length: entries.length },
              () => undefined,
            );
            let next = 0;
            let failed = false;
            const outcomes = await Promise.allSettled(
              Array.from({ length: Math.min(catalogConcurrency, entries.length) }, async () => {
                try {
                  while (!failed && next < entries.length) {
                    const index = next++;
                    const entry = entries[index]!;
                    if (await visible(entry))
                      projected[index] = {
                        ...redactOperationDescription(
                          describeOperation(entry.operation, "lenso.config.ts"),
                        ),
                        schemaVersion: 1,
                        key: entry.key,
                      };
                  }
                } catch (error) {
                  failed = true;
                  throw error;
                }
              }),
            );
            // Drain every in-flight policy before a request owner can clean up.
            for (const outcome of outcomes) if (outcome.status === "rejected") throw outcome.reason;
            const result = projected.filter(
              (entry): entry is ManageCatalogEntry => entry !== undefined,
            );
            current();
            return JSON.parse(boundedJson(result, maxOutputBytes)) as typeof result;
          } catch (error) {
            if (error instanceof EngineError) throw error;
            throw new EngineError(diagnostic(error, { instanceId, phase: "discovery" }), {
              cause: error,
            });
          }
        },
        invokeEntry(key: string, input: unknown) {
          return dispatch(() => {
            const entry = current().byKey.get(key);
            if (!entry)
              refuse("unknown-operation", "Catalog entry is not selected for this adapter.");
            return entry;
          }, input);
        },
        invoke(pluginId: string, method: string, input: unknown) {
          return dispatch(() => {
            const methods = current().byPlugin.get(pluginId);
            if (!methods) refuse("unknown-plugin", "Unknown plugin.");
            const entry = methods.get(method);
            if (!entry)
              refuse(
                "unknown-operation",
                "Service method is not explicitly exposed for this entry.",
              );
            return entry;
          }, input);
        },
      });
    },
  });
}
