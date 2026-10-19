export interface DevConnectionInfo {
  readonly socket: string;
  readonly generation: string;
  readonly instanceId: string;
}

export interface DevReadyInfo {
  readonly connection?: DevConnectionInfo;
  readonly urls?: readonly (string | URL)[];
  readonly capabilities?: readonly string[];
}

export interface DevReadyMessage {
  readonly connection?: DevConnectionInfo;
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
  if ("connection" in message) {
    const value = message.connection;
    if (
      !value ||
      typeof value !== "object" ||
      !["socket", "generation", "instanceId"].every(
        (key) => typeof Reflect.get(value, key) === "string",
      )
    )
      return false;
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
        ...(info.connection ? { connection: info.connection } : {}),
        urls: (info.urls ?? []).map(String),
        capabilities: info.capabilities ?? [],
      },
      () => {},
    );
  } catch {
    // A disconnected supervisor must not fail application startup.
  }
}
