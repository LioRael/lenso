/** Process-local notification; publishing awaits a snapshot of listeners in registration order. */
export class Event<T> {
  private readonly listeners = new Map<number, (value: T) => void | Promise<void>>();
  private nextId = 0;

  constructor(readonly id: string) {}

  /** Removing a listener affects future publications, including nested publications. */
  subscribe(listener: (value: T) => void | Promise<void>): () => void {
    const id = this.nextId++;
    this.listeners.set(id, listener);
    return () => { this.listeners.delete(id); };
  }

  /** The first listener failure stops delivery; completed effects are not rolled back. */
  async publish(value: T): Promise<void> {
    const listeners = [...this.listeners.values()];
    for (const listener of listeners) await listener(value);
  }
}
import { ActionError } from "./error.js";
import type { Task } from "./task.js";


export interface EventDelivery {
  readonly subscription: string;
  readonly task: string;
  readonly payload: string;
}

/** Publishers supply an event value; subscribers own task mapping and durable payload encoding. */
export class DurableEvent<T> {
  private readonly subscriptions = new Map<string, (value: T) => EventDelivery>();

  constructor(readonly id: string) {}

  subscribe<I, O, R>(id: string, task: Task<I, O, R>, input: (event: T) => NoInfer<I>): () => void {
    if (this.subscriptions.has(id)) throw new ActionError("conflict", "Duplicate event subscription");
    const prepare = (value: T): EventDelivery => {
      const payload = input(value);
      try { return { subscription: id, task: task.id, payload: task.input.encode(payload) }; }
      catch (cause) { throw new ActionError("invalid_input", "Invalid event payload", { cause }); }
    };
    this.subscriptions.set(id, prepare);
    return () => { if (this.subscriptions.get(id) === prepare) this.subscriptions.delete(id); };
  }

  deliveries(value: T): readonly EventDelivery[] {
    return [...this.subscriptions.values()].map((prepare) => prepare(value));
  }
}
