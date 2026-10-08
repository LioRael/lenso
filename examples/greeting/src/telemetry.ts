import { bootstrapTelemetry } from "@lenso/otel/bun";

// Explicit finite-command preload; import it before CLI/config/business modules.
await bootstrapTelemetry({ serviceName: "lenso-greeting-cli", flushOnCliExit: true });
