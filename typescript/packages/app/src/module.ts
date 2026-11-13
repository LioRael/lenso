import { ActionError } from "./error.js";
import type { Action } from "./action.js";
import type { Task } from "./task.js";
import type { Event, DurableEvent } from "./events.js";

export interface ModuleExports {
  readonly actions: Readonly<Record<string, Action<unknown, unknown, never>>>;
  readonly tasks?: Readonly<Record<string, Task<unknown, unknown, never>>>;
  readonly events?: Readonly<Record<string, Pick<Event<unknown>, "id" | "subscribe" | "publish"> | Pick<DurableEvent<unknown>, "id" | "subscribe" | "deliveries">>>;
  readonly resources?: readonly string[];
}

export interface ModuleInfo {
  readonly id: string;
  readonly actions: readonly string[];
  readonly tasks: readonly string[];
  readonly events: readonly string[];
  readonly resources: readonly string[];
}

/** Groups native exports without replacing them with a string-based invocation API. */
export class Module<T extends ModuleExports> {
  constructor(readonly id: string, readonly exports: T) {}

  describe(): ModuleInfo {
    return {
      id: this.id,
      actions: Object.values(this.exports.actions).map((action) => action.id),
      tasks: Object.values(this.exports.tasks ?? {}).map((task) => task.id),
      events: Object.values(this.exports.events ?? {}).map((event) => event.id),
      resources: [...(this.exports.resources ?? [])],
    };
  }
}

/** Introspection is derived from installed modules; execution keeps the native handles. */
export class Catalog {
  private readonly modules: ModuleInfo[] = [];

  register(module: Module<ModuleExports>): void {
    const info = module.describe();
    const all = [...this.modules, info];
    for (const key of ["id", "actions", "tasks", "events"] as const) {
      const ids = all.flatMap((item) => item[key]);
      if (new Set(ids).size !== ids.length) {
        throw new ActionError("conflict", `Duplicate module ${key}`);
      }
    }
    this.modules.push(info);
  }

  describe(): readonly ModuleInfo[] {
    return this.modules.map((info) => ({
      ...info, actions: [...info.actions], tasks: [...info.tasks], events: [...info.events], resources: [...info.resources],
    }));
  }

  /** Checks explicit resource requirements before the host starts serving requests. */
  requireResources(available: readonly string[]): void {
    const provided = new Set(available);
    for (const module of this.modules) {
      for (const resource of module.resources) {
        if (!provided.has(resource)) throw new ActionError("not_found", `Missing resource: ${resource}`);
      }
    }
  }
}
