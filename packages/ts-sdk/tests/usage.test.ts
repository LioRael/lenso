import { describe, expect, test } from "vitest";

import {
  createClient,
  type CreateUserResponse,
  LensoApiError,
  type PasswordSessionResponse,
} from "../src/index.js";

describe("createClient", () => {
  test("creates users through the identity API", async () => {
    const requests: Array<{ input: RequestInfo | URL; init?: RequestInit }> =
      [];
    const fetchImpl: typeof fetch = async (input, init) => {
      requests.push(init === undefined ? { input } : { input, init });

      return new Response(
        JSON.stringify({
          data: {
            id: "usr_1",
            email: "ada@example.com",
            display_name: "Ada",
            created_at: "2026-05-31T00:00:00Z",
          },
        }),
        { status: 200 }
      );
    };
    const client = createClient({
      baseUrl: "http://localhost:3000/",
      fetch: fetchImpl,
    });

    const user: CreateUserResponse = await client.identity.createUser({
      email: "ada@example.com",
      display_name: "Ada",
    });

    expect(user).toEqual({
      id: "usr_1",
      email: "ada@example.com",
      display_name: "Ada",
      created_at: "2026-05-31T00:00:00Z",
    });
    expect(requests).toHaveLength(1);
    expect(String(requests[0]?.input)).toBe(
      "http://localhost:3000/v1/identity/users"
    );
    expect(requests[0]?.init?.method).toBe("POST");
    expect(JSON.parse(String(requests[0]?.init?.body))).toEqual({
      email: "ada@example.com",
      display_name: "Ada",
    });
  });

  test("throws API errors for non-success responses", async () => {
    const fetchImpl: typeof fetch = async () =>
      new Response(
        JSON.stringify({
          error: {
            code: "validation_failed",
            message: "Email is invalid",
          },
        }),
        { status: 422 }
      );
    const client = createClient({
      baseUrl: "http://localhost:3000",
      fetch: fetchImpl,
    });

    await expect(
      client.identity.createUser({
        email: "not-an-email",
        display_name: "Ada",
      })
    ).rejects.toBeInstanceOf(LensoApiError);
    await expect(
      client.identity.createUser({
        email: "not-an-email",
        display_name: "Ada",
      })
    ).rejects.toMatchObject({
      status: 422,
      response: {
        error: {
          code: "validation_failed",
          message: "Email is invalid",
        },
      },
    });
  });

  test("creates password auth sessions", async () => {
    const requests: Array<{ input: RequestInfo | URL; init?: RequestInit }> =
      [];
    const fetchImpl: typeof fetch = async (input, init) => {
      requests.push(init === undefined ? { input } : { input, init });

      return new Response(
        JSON.stringify({
          data: {
            user_id: "usr_1",
            session_id: "sess_1",
            token: "sess_token",
            expires_at: "2026-06-16T12:00:00Z",
          },
        }),
        { status: 200 }
      );
    };
    const client = createClient({
      baseUrl: "http://localhost:3000/",
      fetch: fetchImpl,
    });

    const registerSession: PasswordSessionResponse =
      await client.auth.password.register({
        identifier: "ada@example.com",
        password: "correct horse",
      });
    const loginSession = await client.auth.password.login({
      identifier: "ada@example.com",
      password: "correct horse",
    });

    expect(registerSession).toEqual({
      user_id: "usr_1",
      session_id: "sess_1",
      token: "sess_token",
      expires_at: "2026-06-16T12:00:00Z",
    });
    expect(loginSession).toEqual(registerSession);
    expect(requests.map((request) => String(request.input))).toEqual([
      "http://localhost:3000/v1/auth/password/register",
      "http://localhost:3000/v1/auth/password/login",
    ]);
  });
});
