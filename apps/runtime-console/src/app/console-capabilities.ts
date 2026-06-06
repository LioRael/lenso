import { apiAuthToken, isApiMode } from "../lib/http-client";

const localConsoleCapabilities = ["runtime.stories.read"] as const;

export function parseDevAuthTokenScopes(token: string): string[] {
  const normalized = token.startsWith("Bearer ")
    ? token.slice("Bearer ".length)
    : token;
  const serviceToken = normalized.startsWith("dev-service:")
    ? normalized.slice("dev-service:".length)
    : normalized.startsWith("dev-user:")
      ? normalized.slice("dev-user:".length)
      : normalized;
  const [, rawScopes] = serviceToken.split(":", 2);
  if (!rawScopes) {
    return [];
  }
  return rawScopes.split(",").filter((scope) => scope.length > 0);
}

export function consoleCapabilityProvider({
  apiMode = isApiMode(),
  authToken = apiAuthToken,
}: {
  apiMode?: boolean;
  authToken?: string;
} = {}): readonly string[] {
  if (!apiMode) {
    return localConsoleCapabilities;
  }
  return parseDevAuthTokenScopes(authToken);
}

export function useConsoleCapabilities(): readonly string[] {
  return consoleCapabilityProvider();
}
