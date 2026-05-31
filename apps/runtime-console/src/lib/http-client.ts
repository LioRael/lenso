import ky, { isHTTPError } from "ky";

const apiBaseUrl = import.meta.env.VITE_API_BASE_URL as string | undefined;

export const httpClient = ky.create({
  ...(apiBaseUrl ? { prefix: apiBaseUrl.replace(/\/$/, "") } : {}),
  hooks: {
    beforeRequest: [
      ({ request }) => {
        request.headers.set("Accept", "application/json");
        request.headers.set("Authorization", "Bearer dev-service:admin");
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
