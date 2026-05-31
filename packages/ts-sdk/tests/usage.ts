import { createClient, type CreateUserResponse, LensoApiError } from '../src/index.js';

const client = createClient({
  baseUrl: 'http://localhost:3000',
  fetch: async () =>
    new Response(
      JSON.stringify({
        data: {
          id: 'usr_1',
          email: 'ada@example.com',
          display_name: 'Ada',
          created_at: '2026-05-31T00:00:00Z',
        },
      }),
      { status: 200 },
    ),
});

const user: Promise<CreateUserResponse> = client.identity.createUser({
  email: 'ada@example.com',
  display_name: 'Ada',
});

try {
  await user;
} catch (error) {
  if (error instanceof LensoApiError) {
    console.log(error.response.error.code);
  }
}
