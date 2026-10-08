export interface DevReadyInfo {
  readonly urls?: readonly (string | URL)[];
  readonly capabilities?: readonly string[];
}

/** Report successful startup to lenso dev; a no-op without its IPC channel. */
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
    // Presentation must never fail application startup when the dev parent disconnects.
  }
}
