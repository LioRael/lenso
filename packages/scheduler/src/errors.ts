export class SchedulerError extends Error {
  constructor(
    readonly code:
      | "invalid-input"
      | "invalid-options"
      | "unsupported-storage"
      | "queue-mismatch"
      | "not-found"
      | "conflict"
      | "cancelled"
      | "forbidden",
  ) {
    super(`Scheduler ${code}`);
    this.name = "SchedulerError";
  }
}
