import { createWebhookReceiver, type WebhookInbox } from "@lenso/webhooks/receiver";
import type { SigningKey } from "@lenso/webhooks";

/** Mount handle on the host's existing Web listener. No resource is acquired by this factory. */
export function receiverExample(input: {
  inbox: WebhookInbox;
  keys: () => Promise<readonly SigningKey[]>;
}) {
  return createWebhookReceiver({
    receiverId: "orders-partner-tenant-a",
    source: "orders-service",
    eventTypes: ["order.completed"],
    maxBodyBytes: 262144,
    bodyTimeoutMs: 5000,
    toleranceSeconds: 300,
    inbox: input.inbox,
    keys: input.keys,
  });
}
