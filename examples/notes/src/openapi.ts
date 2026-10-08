import { openapi } from "@orpc/openapi";
import { createOpenAPIAdapter } from "@lenso/web/openapi";
import type { FetchAuthContext } from "@lenso/auth/fetch";
import type { NotesRouter } from "./router";

/** Optional selection; importing this factory does not mount an HTTP or docs route. */
export function createNotesOpenAPI(router: NotesRouter) {
  const selectedRouter = {
    list: router.list.meta(openapi({ method: "GET", path: "/notes" })),
    read: router.read.meta(openapi({ method: "POST", path: "/notes/read" })),
  };
  const adapter = createOpenAPIAdapter<FetchAuthContext>({
    selectedRouter,
    prefix: "/api",
    // The selected procedures already verify request evidence through requiredAuth.
    authenticate: () => {},
  });
  return { ...adapter, selectedRouter };
}
