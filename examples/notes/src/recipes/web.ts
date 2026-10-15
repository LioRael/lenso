import { AuthError } from "@lenso/auth";
import { authErrorResponse } from "@lenso/auth/fetch";
import type { Plugin } from "@lenso/core";
import type { Files } from "@lenso/storage/files";
import { createWebPlugin } from "@lenso/web";
import type { NotesAuthentication } from "../auth";
import type { NotesFileAccess } from "../private-files";
import { createLoginRoutes, RecipeInputError, sessionPaths } from "./login";
import { createPrivateFileRoutes, filesPath } from "./private-files";

/** Ordinary, editable HTTP wiring. Remove either entry without changing the borrowed services. */
export function createRecipeWeb(options: {
  authentication: Plugin<NotesAuthentication>;
  login?: boolean;
  files?: { plugin: Plugin<Files<NotesFileAccess>>; storageId: string };
}) {
  const { authentication, files } = options;
  const login = options.login ?? true;
  const web = createWebPlugin({
    id: "recipe-web",
    requires: [authentication, ...(files ? [files.plugin] : [])],
    router: () => ({}),
    fetch(context) {
      const auth = context.get(authentication);
      const loginRoutes = login ? createLoginRoutes(auth) : undefined;
      const fileRoutes = files
        ? createPrivateFileRoutes({
            auth,
            files: context.get(files.plugin),
            storageId: files.storageId,
          })
        : undefined;
      return async (request) => {
        try {
          return (await loginRoutes?.(request)) ?? (await fileRoutes?.(request));
        } catch (error) {
          request.signal.throwIfAborted();
          if (error instanceof AuthError) return authErrorResponse(error);
          if (error instanceof RecipeInputError)
            return Response.json({ code: "BAD_REQUEST", message: error.message }, { status: 400 });
          return Response.json({ code: "SERVICE_UNAVAILABLE" }, { status: 503 });
        }
      };
    },
  });
  return {
    ...web,
    source: { file: "src/recipes/web.ts", export: "createRecipeWeb" },
    contributions: [
      ...(login
        ? Object.values(sessionPaths).map((path) => ({
            kind: "recipe-http-entry",
            capability: "login",
            provider: authentication.id,
            method: "POST",
            path,
          }))
        : []),
      ...(files
        ? [
            {
              kind: "recipe-http-entry",
              capability: "private-files",
              provider: files.plugin.id,
              method: "PUT",
              path: filesPath,
            },
            ...["GET", "HEAD", "DELETE"].map((method) => ({
              kind: "recipe-http-entry",
              capability: "private-files",
              provider: files.plugin.id,
              method,
              path: `${filesPath}/:fileId`,
            })),
          ]
        : []),
    ],
  };
}
