import { WebhookError, type WebhookConfig } from "./contracts";
import { validateEndpointUrl } from "./network";

export function webhookConfig(input: Partial<WebhookConfig> & Pick<WebhookConfig, "instanceId" | "source" | "eventTypes" | "outbound">): WebhookConfig {
  const config = {
    enabled: false,
    maxAttempts: 5,
    baseDelayMs: 1000,
    maxDelayMs: 3_600_000,
    concurrency: 4,
    retentionMs: 30 * 86_400_000,
    ...input,
  };
  const bounded = (value: number, min: number, max: number) =>
    Number.isSafeInteger(value) && value >= min && value <= max;
  if (
    typeof config.enabled !== "boolean" ||
    typeof config.instanceId !== "string" || !config.instanceId || config.instanceId.length > 128 ||
    typeof config.source !== "string" || !config.source || config.source.length > 256 ||
    !Array.isArray(config.eventTypes) || config.eventTypes.length < 1 || config.eventTypes.length > 1000 ||
    config.eventTypes.some(type => !/^[a-zA-Z][a-zA-Z0-9_.-]{0,127}$/.test(type)) ||
    new Set(config.eventTypes).size !== config.eventTypes.length ||
    !bounded(config.maxAttempts, 1, 100) || !bounded(config.concurrency, 1, 100) ||
    !bounded(config.baseDelayMs, 100, 86_400_000) ||
    !bounded(config.maxDelayMs, config.baseDelayMs, 604_800_000) ||
    !bounded(config.retentionMs, 86_400_000, 31_622_400_000) ||
    !config.outbound ||
    !bounded(config.outbound.timeoutMs, 100, 300_000) ||
    !bounded(config.outbound.dnsTimeoutMs, 1, config.outbound.timeoutMs) ||
    !bounded(config.outbound.connectTimeoutMs, 1, config.outbound.timeoutMs) ||
    !bounded(config.outbound.maxRequestBytes, 1, 1_048_576) ||
    !bounded(config.outbound.maxResponseBytes, 1, 1_048_576) ||
    !Array.isArray(config.outbound.allowedHosts) || !config.outbound.allowedHosts.length
  ) throw new WebhookError("invalid-config");
  try {
    for (const host of config.outbound.allowedHosts) validateEndpointUrl(`https://${host}/`, config.outbound);
  } catch { throw new WebhookError("invalid-config"); }
  return Object.freeze({
    ...config,
    eventTypes: Object.freeze([...config.eventTypes]),
    outbound: Object.freeze({ ...config.outbound, allowedHosts: Object.freeze([...config.outbound.allowedHosts]) }),
  });
}
