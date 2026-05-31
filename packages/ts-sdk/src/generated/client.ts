/* eslint-disable */
// Generated from contracts/openapi/app-api.v1.yaml. Do not edit by hand.

import type { CreateUserRequest, CreateUserResponse, ErrorResponse } from './types.js';

export type LensoClientOptions = {
  baseUrl: string;
  fetch?: typeof fetch;
  headers?: HeadersInit | (() => HeadersInit | Promise<HeadersInit>);
};

export class LensoApiError extends Error {
  readonly status: number;
  readonly response: ErrorResponse;

  constructor(status: number, response: ErrorResponse) {
    super(response.error.message);
    this.name = 'LensoApiError';
    this.status = status;
    this.response = response;
  }
}

export type CreateUserResponseEnvelope = {
  data: CreateUserResponse;
};

export class GeneratedLensoClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;
  private readonly headers?: LensoClientOptions['headers'];

  constructor(options: LensoClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, '');
    this.fetchImpl = options.fetch ?? fetch;
    this.headers = options.headers;
  }

  async createUser(input: CreateUserRequest): Promise<CreateUserResponse> {
    const response = await this.fetchImpl(`${this.baseUrl}/v1/identity/users`, {
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        ...(await this.resolveHeaders()),
      },
      body: JSON.stringify(input),
    });

    const body = await response.json();
    if (!response.ok) {
      throw new LensoApiError(response.status, body as ErrorResponse);
    }

    return (body as CreateUserResponseEnvelope).data;
  }

  private async resolveHeaders(): Promise<HeadersInit> {
    if (!this.headers) {
      return {};
    }

    return typeof this.headers === 'function' ? await this.headers() : this.headers;
  }
}
