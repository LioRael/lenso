import { RealtimeError, type RealtimeConfig } from "./contracts";

export const defaults: Required<RealtimeConfig> = {
  maxConnections: 1000,
  maxConnectionsPerSubject: 8,
  maxSubscriptionsPerConnection: 16,
  maxSubscriptionsPerSubject: 64,
  maxSubscribersPerTopic: 1000,
  maxTopics: 1000,
  maxPendingOperations: 64,
  maxPayloadBytes: 16384,
  maxBufferedEvents: 64,
  maxBufferedBytes: 131072,
  authorizationLeaseMs: 30000,
  sweepMs: 250,
  heartbeatMs: 15000,
  retryMs: 2000,
  cursorMaxAgeMs: 600000,
  snapshotTimeoutMs: 5000,
};

export function resolveRealtimeConfig(input: RealtimeConfig = {}): Required<RealtimeConfig> {
  const config = { ...defaults, ...input };
  for (const key of Object.keys(config) as (keyof RealtimeConfig)[]) {
    const value = config[key];
    if (!(key in defaults) || !Number.isSafeInteger(value) || value < 1 || value > 10000000)
      throw new RealtimeError("invalid-input");
  }
  if (
    config.authorizationLeaseMs > 30000 ||
    config.sweepMs > 1000 ||
    config.sweepMs > config.authorizationLeaseMs ||
    config.maxPayloadBytes > 32768 ||
    config.maxBufferedBytes < 512 ||
    config.maxBufferedBytes < config.maxPayloadBytes + 512
  )
    throw new RealtimeError("invalid-input");
  return config;
}
