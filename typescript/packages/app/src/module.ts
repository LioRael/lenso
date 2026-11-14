import { ActionError } from "./error.js";
import type { Action } from "./action.js";
import type { Task } from "./task.js";
import type { Event, DurableEvent } from "./events.js";
import type { ActionDescription, ActionMetadata, ModuleMetadata } from "./metadata.js";
import { describeAction } from "./metadata.js";

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
  readonly metadata?: ModuleMetadata | undefined;
  readonly actionMetadata?: Readonly<Record<string, ActionMetadata>> | undefined;
}

/** Groups native exports without replacing them with a string-based invocation API. */
export class Module<T extends ModuleExports> {
  constructor(readonly id: string, readonly exports: T, readonly metadata?: ModuleMetadata) {}

  describe(): ModuleInfo {
    const descriptions = Object.values(this.exports.actions).map(describeAction);
    const actionMetadata = Object.fromEntries(descriptions.flatMap(({ id, metadata }) => metadata === null ? [] : [[id, metadata]]));
    return {
      id: this.id,
      actions: descriptions.map(({ id }) => id),
      tasks: Object.values(this.exports.tasks ?? {}).map((task) => task.id),
      events: Object.values(this.exports.events ?? {}).map((event) => event.id),
      resources: [...(this.exports.resources ?? [])],
      ...(this.metadata === undefined ? {} : { metadata: structuredClone(this.metadata) }),
      ...(Object.keys(actionMetadata).length ? { actionMetadata } : {}),
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
    return structuredClone(this.modules);
  }

  describeActions(): readonly ActionDescription[] { return describeActions(this.modules); }

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

/** Shared by native Catalogs and clients consuming HTTP/Console module descriptions. */
export function describeActions(modules: readonly ModuleInfo[]): ActionDescription[] {
  return modules.flatMap((module) => module.actions.map((id) => {
    const metadata = module.actionMetadata && Object.hasOwn(module.actionMetadata, id) ? module.actionMetadata[id] : undefined;
    return { id, metadata: metadata === undefined ? null : structuredClone(metadata) };
  }));
}
