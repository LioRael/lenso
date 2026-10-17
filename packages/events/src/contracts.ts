export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

export interface Event {
  readonly id: string;
  readonly topic: string;
  readonly payload: Json;
}

export interface Delivery {
  readonly event: Event;
  readonly subscriptionId: string;
  /** Stable across attempts; pass to an idempotent destination. */
  readonly idempotencyKey: string;
  readonly attempt: number;
  readonly token: string;
}

export interface DeliveryStatus {
  subscriptionId: string;
  state: "pending" | "processing" | "delivered" | "cancelled";
  attempts: number;
  availableAt: number;
  leaseUntil: number | null;
}

/** All state transitions are atomic. A store is borrowed, never closed by Events. */
export interface EventStore {
  register(subscriptionId: string, topic: string): void;
  cancel(subscriptionId: string): void;
  /** Atomically persist the event and fan out to currently active subscriptions. */
  publish(event: Event): { created: boolean };
  claim(subscriptionIds: readonly string[], leaseMs: number): Delivery | null;
  /** Return false when the claim expired, was replaced, or was cancelled. */
  acknowledge(delivery: Delivery): boolean;
  retry(delivery: Delivery, delayMs: number): boolean;
  status(eventId: string): DeliveryStatus[];
}

export type EventHandler = (
  event: Event,
  context: { subscriptionId: string; idempotencyKey: string; attempt: number; signal: AbortSignal },
) => void | Promise<void>;

export function name(value: string): void {
  if (typeof value !== "string" || !value.trim() || value.length > 256)
    throw new Error("Event IDs and topics must contain 1–256 characters");
}

export function duration(value: number): void {
  if (!Number.isSafeInteger(value) || value < 1 || value > 86_400_000)
    throw new Error("Event durations must be integers between 1 and 86400000 ms");
}
