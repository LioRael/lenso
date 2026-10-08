export interface DevReadyInfo {
  readonly urls?: readonly (string | URL)[];
  readonly capabilities?: readonly string[];
}

export interface DevReadyMessage {
  readonly type: "lenso:dev-ready";
  readonly urls?: readonly string[];
  readonly capabilities?: readonly string[];
}

export function isDevReadyMessage(message: unknown): message is DevReadyMessage {
  if (!message || typeof message !== "object" || !("type" in message)) return false;
  if (message.type !== "lenso:dev-ready") return false;
  for (const key of ["urls", "capabilities"] as const) {
    if (key in message) {
      const values: unknown = Reflect.get(message, key);
      if (!Array.isArray(values) || !values.every((value) => typeof value === "string"))
        return false;
    }
  }
  return true;
}

/** Report successful startup; a no-op without the development IPC channel. */
export function reportDevReady(info: DevReadyInfo = {}): void {
  if (!process.send || process.connected === false) return;
  try {
    process.send(
      {
        type: "lenso:dev-ready",
        urls: (info.urls ?? []).map(String),
        capabilities: info.capabilities ?? [],
      },
      () => {},
    );
  } catch {
    // A disconnected supervisor must not fail application startup.
  }
}
