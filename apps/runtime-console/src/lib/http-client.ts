import ky, { isHTTPError } from "ky";

const apiBaseUrl = import.meta.env.VITE_API_BASE_URL as string | undefined;
const runtimeConsoleMode = import.meta.env.VITE_RUNTIME_CONSOLE_MODE as
  | "api"
  | "mock"
  | undefined;
export const apiAuthToken =
  (import.meta.env.VITE_API_AUTH_TOKEN as string | undefined) ??
  "dev-service:admin:runtime.stories.read,identity.users.read,remote_crm.contacts.read,remote_crm.contacts.sync";

export function isApiMode() {
  return runtimeConsoleMode === "api" && Boolean(apiBaseUrl);
}

export function runtimeConsoleDataSource() {
  return isApiMode() ? "api" : "mock";
}

export const httpClient = ky.create({
  ...(apiBaseUrl ? { prefix: apiBaseUrl.replace(/\/$/, "") } : {}),
  hooks: {
    beforeRequest: [
      ({ request }) => {
        request.headers.set("Accept", "application/json");
        request.headers.set("Authorization", `Bearer ${apiAuthToken}`);
      },
    ],
    beforeError: [
      async ({ error }) => {
        if (!isHTTPError(error)) {
          return error;
        }

        const body = await error.response.json().catch(() => undefined);
        if (
          body &&
          typeof body === "object" &&
          "error" in body &&
          body.error &&
          typeof body.error === "object" &&
          "message" in body.error
        ) {
          error.message = String(body.error.message);
        }
        return error;
      },
    ],
  },
});
