import createClient, { type Client, type ClientOptions, type Middleware } from 'openapi-fetch';

export type BearerAuthentication = {
  readonly kind: 'bearer';
  readonly accessToken: () => string | undefined | Promise<string | undefined>;
};

export type SessionAuthentication = {
  readonly kind: 'session';
  readonly csrfHeader?: string;
  readonly csrfToken: () => string | undefined | Promise<string | undefined>;
};

export type BrowserAuthentication = BearerAuthentication | SessionAuthentication;

export type LensoWebClientOptions = Omit<ClientOptions, 'baseUrl'> & {
  readonly baseUrl: string;
  readonly authentication?: BrowserAuthentication;
};

export type ProblemDetails = {
  readonly type?: string;
  readonly title?: string;
  readonly status?: number;
  readonly detail?: string;
  readonly instance?: string;
  readonly code?: string;
  readonly [key: string]: unknown;
};

export class LensoApiError extends Error {
  readonly problem: ProblemDetails;
  readonly response: Response;

  constructor(response: Response, problem: ProblemDetails) {
    super(problem.detail ?? problem.title ?? `Lenso Web request failed with HTTP ${response.status}`);
    this.name = 'LensoApiError';
    this.problem = problem;
    this.response = response;
  }
}

export class LensoTransportError extends Error {
  readonly cause: unknown;

  constructor(cause: unknown) {
    super(cause instanceof Error ? cause.message : 'Lenso Web transport failed');
    this.name = 'LensoTransportError';
    this.cause = cause;
  }
}

export type LensoResult<Data, ErrorBody = unknown> =
  | { readonly data: Data; readonly error?: never; readonly response: Response }
  | { readonly data?: never; readonly error: ErrorBody; readonly response: Response };

export const browserSupport = Object.freeze({
  requestResponse: true,
  responseStream: true,
  webSocket: false,
  workersHost: false,
});

export function createLensoWebClient<Paths extends {}>(options: LensoWebClientOptions): Client<Paths> {
  const { authentication, ...clientOptions } = options;
  const authenticatedOrigin = authentication === undefined ? undefined : configuredOrigin(options.baseUrl);
  const client = createClient<Paths>(authentication?.kind === 'session'
    ? { ...clientOptions, credentials: 'include' }
    : clientOptions);
  client.use(authenticationMiddleware(authentication, authenticatedOrigin), transportErrorMiddleware);
  return client;
}

export function unwrap<Data>(result: LensoResult<Data>): Data {
  if ('error' in result) throw new LensoApiError(result.response, asProblem(result.error, result.response.status));
  return result.data;
}

export function unwrapStream(result: LensoResult<ReadableStream<Uint8Array> | null>): ReadableStream<Uint8Array> {
  const stream = unwrap(result);
  if (stream === null) throw new LensoTransportError(new Error('The response did not include a readable body stream'));
  return stream;
}

function authenticationMiddleware(authentication: BrowserAuthentication | undefined, authenticatedOrigin: string | undefined): Middleware {
  return {
    async onRequest({ request }) {
      if (authenticatedOrigin !== undefined && new URL(request.url).origin !== authenticatedOrigin) {
        throw new Error('Authenticated Lenso Web requests cannot override the configured origin');
      }
      if (authentication?.kind === 'bearer') {
        const token = await authentication.accessToken();
        if (token !== undefined) request.headers.set('authorization', `Bearer ${token}`);
      } else if (authentication?.kind === 'session' && isUnsafeMethod(request.method)) {
        const token = await authentication.csrfToken();
        if (token === undefined || token.length === 0) {
          throw new Error('Session-authenticated mutation requires a CSRF token');
        }
        request.headers.set(authentication.csrfHeader ?? 'x-csrf-token', token);
      }
      return request;
    },
  };
}

function configuredOrigin(baseUrl: string): string {
  const url = new URL(baseUrl);
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) {
    throw new Error('Authenticated Lenso Web baseUrl must be an HTTP(S) origin without credentials');
  }
  return url.origin;
}

const transportErrorMiddleware: Middleware = {
  onError({ error }) {
    return new LensoTransportError(error);
  },
};

function isUnsafeMethod(method: string): boolean {
  return !['GET', 'HEAD', 'OPTIONS'].includes(method.toUpperCase());
}

function asProblem(value: unknown, status: number): ProblemDetails {
  if (typeof value === 'object' && value !== null && !Array.isArray(value)) {
    const problem = value as Record<string, unknown>;
    return { ...problem, status: typeof problem.status === 'number' ? problem.status : status };
  }
  return typeof value === 'string'
    ? { status, title: `HTTP ${status}`, detail: value }
    : { status, title: `HTTP ${status}` };
}
