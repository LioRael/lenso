import { expect, test } from 'bun:test';
import { os, ORPCError } from '@orpc/server';
import { defineApp, definePlugin, startApp } from 'lenso';
import { z } from 'zod';
import { createWebPlugin, type WebContext } from '../src/index';
import { createClient } from '../src/client';

test('real HTTP typed client calls an initialized async dependency', async () => {
  const service = definePlugin({
    id: 'business',
    setup: () => ({ greet: async ({ name }: { name: string }) => `Hello, ${name}!` }),
  });
  const router = (greet: (input: { name: string }) => Promise<string>) => ({
    greet: os.$context<WebContext>().input(z.object({ name: z.string() })).handler(({ input }) => greet(input)),
  });
  const web = createWebPlugin({ requires: [service], router: context => router(context.get(service).greet) });
  const app = await startApp(defineApp({ plugins: [web, service] }));
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: app.get(web).fetch });
  try {
    const client = createClient<ReturnType<typeof router>>(new URL('/rpc', server.url));
    const output: string = await client.greet({ name: 'Ada' });
    expect(output).toBe('Hello, Ada!');
    expect((await fetch(new URL('/missing', server.url))).status).toBe(404);
    if (false) {
      // This assertion is checked by typecheck; the invalid call never runs.
      // @ts-expect-error The actual inferred router accepts a string name only.
      await client.greet({ name: 42 });
    }
  } finally {
    await server.stop(true);
    await app.stop();
  }
});

test('optional oRPC auth middleware composes without core auth conventions', async () => {
  const procedure = os.$context<WebContext>().use(async ({ context, next }) => {
    if (context.request.headers.get('x-development-token') !== 'local-test') {
      throw new ORPCError('UNAUTHORIZED');
    }
    return next();
  });
  const router = { protected: procedure.handler(() => ({ allowed: true })) };
  const web = createWebPlugin({ requires: [], router: () => router });
  const app = await startApp(defineApp({ plugins: [web] }));
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: app.get(web).fetch });
  try {
    const url = new URL('/rpc', server.url);
    await expect(createClient<typeof router>(url).protected()).rejects.toMatchObject({ code: 'UNAUTHORIZED' });
    expect(await createClient<typeof router>(url, { headers: { 'x-development-token': 'local-test' } }).protected()).toEqual({ allowed: true });
  } finally {
    await server.stop(true);
    await app.stop();
  }
});
