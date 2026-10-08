export class NotificationError extends Error {
  constructor(
    readonly code:
      | "invalid-input"
      | "invalid-template"
      | "idempotency-conflict"
      | "channel-unavailable"
      | "delivery-busy"
      | "delivery-retry"
      | "access-denied",
  ) {
    super(`Notification operation failed: ${code}`);
    this.name = "NotificationError";
  }
}
