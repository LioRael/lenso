import { validatePlugins, type Plugin, type RunningApp } from "@lenso/core";
import {
  describeOperation,
  boundedJson,
  invokeValidatedOperation,
  operationError,
  redactOperationDescription,
  resolveOperation,
  validateOperationInput,
  validateOperations,
  type Operation,
  type OperationBoundOptions,
} from "@lenso/engine/operations";
import { diagnostic, EngineError } from "@lenso/engine/diagnostics";

export interface ManageInvocationBinding<C = unknown> {
  readonly context?: C;
  readonly confirm?: () => boolean | Promise<boolean>;
  readonly approve?: () => boolean | Promise<boolean>;
}

export interface ManageAdapterOptions<O extends Operation = Operation> {
  readonly running: RunningApp;
  readonly plugins: readonly Plugin<unknown>[];
  readonly operations: readonly O[];
  readonly binding: (
    operation: O,
    validatedInput: unknown,
  ) => OperationBoundOptions<NoInfer<O>> | Promise<OperationBoundOptions<NoInfer<O>>>;
  readonly canList: (operation: O) => boolean | Promise<boolean>;
  readonly maxOutputBytes?: number;
}

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
  const { running, binding, canList, maxOutputBytes } = options;
  const plugins = Object.freeze([...options.plugins]);
  const operations = Object.freeze([...options.operations]);
  validatePlugins(plugins);
  validateOperations(plugins, operations);
  for (const operation of operations) {
    try {
      running.get(operation.plugin);
    } catch (cause) {
      throw new EngineError(
        {
          code: "invalid-manage",
          phase: "discovery",
          message: "Selected operation must belong to the exact running plugin instance.",
          instanceId: running.instanceId,
          pluginId: operation.plugin.id,
          operation: `${operation.plugin.id}.${operation.method}`,
        },
        { cause },
      );
    }
  }
  if (typeof binding !== "function" || typeof canList !== "function")
    refuse("invalid-manage", "Manage requires invocation binding and caller catalog policy.");
  if (maxOutputBytes !== undefined && (!Number.isSafeInteger(maxOutputBytes) || maxOutputBytes < 1))
    refuse("invalid-manage", "Output byte limit must be a positive safe integer.");
  return Object.freeze({
    async catalog() {
      try {
        const result: ManageCatalogEntry[] = [];
        for (const [index, operation] of operations.entries()) {
          if ((await canList(operation)) === true)
            result.push({
              ...redactOperationDescription(describeOperation(operation, "lenso.config.ts")),
              schemaVersion: 1,
              key: `operation_${index}`,
            });
        }
        return JSON.parse(boundedJson(result, maxOutputBytes)) as typeof result;
      } catch (error) {
        if (error instanceof EngineError) throw error;
        throw new EngineError(
          diagnostic(error, { instanceId: running.instanceId, phase: "discovery" }),
          { cause: error },
        );
      }
    },
    async invokeEntry(key: string, input: unknown) {
      const match = /^operation_(0|[1-9][0-9]*)$/.exec(key);
      const operation = match ? operations[Number(match[1])] : undefined;
      if (!operation)
        throw new EngineError({
          code: "unknown-operation",
          phase: "invoke",
          message: "Catalog entry is not selected for this adapter.",
          instanceId: running.instanceId,
        });
      // Dispatch uses the original declaration, never redacted presentation identifiers.
      return this.invoke(operation.plugin.id, operation.method, input);
    },
    async invoke(pluginId: string, method: string, input: unknown) {
      const location = {
        instanceId: running.instanceId,
        pluginId,
        operation: `${pluginId}.${method}`,
      };
      try {
        const operation = resolveOperation(plugins, operations, pluginId, method) as O;
        if ((await canList(operation)) !== true)
          refuse("forbidden-operation", "Operation is not available to this caller.");
        const validated = await validateOperationInput(operation, input);
        let invocation;
        try {
          invocation = await binding(operation, validated);
        } catch (error) {
          throw operationError(operation, error);
        }
        if (!invocation || typeof invocation !== "object")
          refuse("invalid-manage-binding", "Invocation binding must return a trusted binding.");
        return await invokeValidatedOperation<Operation>(running, operation, validated, {
          context: invocation.context,
          ...(invocation.confirm === undefined ? {} : { confirm: invocation.confirm }),
          ...(invocation.approve === undefined ? {} : { approve: invocation.approve }),
          ...(maxOutputBytes === undefined ? {} : { maxOutputBytes }),
        });
      } catch (error) {
        if (error instanceof EngineError) throw error;
        throw new EngineError(diagnostic(error, { ...location, phase: "invoke" }), {
          cause: error,
        });
      }
    },
  });
}
