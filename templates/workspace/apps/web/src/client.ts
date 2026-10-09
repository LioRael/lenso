import type { AppClient, AppRouter } from "@app/server/client";
import { createClient } from "@lenso/web/client";

export function createAppClient(rpcURL: string | URL): AppClient {
  return createClient<AppRouter>(rpcURL);
}
