import type { Catalog, ModuleInfo } from "./module.js";
import { ActionError } from "./error.js";
import type { Action, Codec, Context, Executor } from "./action.js";

export interface ConsoleOperation<R> {
  readonly id: string;
  readonly invoke: (executor: Executor, context: Context<R>, input: unknown) => Promise<unknown>;
}

/** Binds an existing action to an external interface; validation and authorization stay in its executor. */
export function consoleOperation<I, O, R>(action: Action<I, O, R>, codec?: Codec<NoInfer<I>, NoInfer<O>>): ConsoleOperation<R> {
  return { id: action.id, invoke: (executor, context, input) => executor.callExternal(action, context, input, codec) };
}

export type ConsoleBinding<R> = Action<unknown, unknown, R> | ConsoleOperation<R>;

export interface ConsolePageDefinition<R> {
  readonly id: string;
  readonly title: string;
  readonly list: ConsoleBinding<R>;
  readonly detail?: ConsoleBinding<R>;
  readonly actions?: readonly ConsoleBinding<R>[];
}

/** One page definition supplies both its public description and its authorized management bindings. */
export class ConsoleExtension<R> {
  private readonly operations = new Map<string, ConsoleOperation<R>>();
  private readonly pages: readonly ConsolePage[];

  constructor(private readonly executor: Executor, definitions: readonly ConsolePageDefinition<R>[], operations: readonly ConsoleBinding<R>[] = []) {
    const bindings = new Map<Action<unknown, unknown, R>, ConsoleOperation<R>>();
    const bind = (value: ConsoleBinding<R>): ConsoleOperation<R> => {
      if ("invoke" in value) return value;
      let binding = bindings.get(value);
      if (!binding) { binding = consoleOperation(value); bindings.set(value, binding); }
      return binding;
    };
    const install = (value: ConsoleBinding<R>) => {
      const operation = bind(value);
      const existing = this.operations.get(operation.id);
      if (existing && existing !== operation) throw new ActionError("conflict", `Conflicting Console action: ${operation.id}`);
      this.operations.set(operation.id, operation);
    };
    if (new Set(definitions.map((page) => page.id)).size !== definitions.length) {
      throw new ActionError("conflict", "Duplicate Console page");
    }
    this.pages = definitions.map((page) => {
      for (const operation of [page.list, ...(page.detail ? [page.detail] : []), ...(page.actions ?? [])]) install(operation);
      return { id: page.id, title: page.title, list: page.list.id,
        ...(page.detail ? { detail: page.detail.id } : {}),
        ...(page.actions ? { actions: page.actions.map((action) => action.id) } : {}),
      };
    });
    for (const operation of operations) install(operation);
  }

  manifest(catalog: Catalog): ConsoleManifest {
    return makeManifest(catalog, this.pages, this.operations.keys());
  }

  async call(context: Context<R>, id: string, input: unknown): Promise<unknown> {
    const operation = this.operations.get(id);
    if (!operation) throw new ActionError("not_found", "Management action not found");
    return operation.invoke(this.executor, context, input);
  }
}

/** References existing authorized actions; a page never supplies another execution handler. */
export interface ConsolePage {
  readonly id: string;
  readonly title: string;
  readonly list: string;
  readonly detail?: string;
  readonly actions?: readonly string[];
}

export interface ConsoleManifest {
  readonly version: 1;
  readonly modules: readonly ModuleInfo[];
  readonly pages: readonly ConsolePage[];
}

/** Expose this description only through an authorized management endpoint. */
export function consoleManifest(catalog: Catalog, pages: readonly ConsolePage[]): ConsoleManifest {
  return makeManifest(catalog, pages, []);
}

/** Validates typed protocol invariants after a transport has checked the JSON shape. */
export function validateConsoleManifest(manifest: ConsoleManifest): void {
  validateManifest(manifest, []);
}

function makeManifest(catalog: Catalog, pages: readonly ConsolePage[], operations: Iterable<string>): ConsoleManifest {
  const manifest: ConsoleManifest = { version: 1, modules: catalog.describe(), pages: pages.map((page) => ({
    ...page, ...(page.actions ? { actions: [...page.actions] } : {}),
  })) };
  validateManifest(manifest, operations);
  return manifest;
}

function validateManifest({ modules, pages }: ConsoleManifest, operations: Iterable<string>): void {
  for (const key of ["id", "actions", "tasks", "events"] as const) {
    const values = modules.flatMap((module) => module[key]);
    if (new Set(values).size !== values.length) throw new ActionError("conflict", `Duplicate module ${key}`);
  }
  const actions = new Set(modules.flatMap((module) => module.actions));
  const ids = new Set<string>();
  for (const page of pages) {
    if (ids.has(page.id)) throw new ActionError("conflict", "Duplicate Console page");
    ids.add(page.id);
    for (const action of [page.list, ...(page.detail === undefined ? [] : [page.detail]), ...(page.actions ?? [])]) {
      if (!actions.has(action)) throw new ActionError("not_found", `Unknown Console action: ${action}`);
    }
  }
  for (const id of operations) {
    if (!actions.has(id)) throw new ActionError("not_found", `Unknown Console action: ${id}`);
  }
}
