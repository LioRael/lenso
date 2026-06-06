export function consoleCapabilityProvider(): readonly string[] {
  return ["runtime.stories.read"];
}

export function useConsoleCapabilities(): readonly string[] {
  return consoleCapabilityProvider();
}
