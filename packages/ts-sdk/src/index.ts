import { GeneratedLensoClient, type LensoClientOptions } from './generated/client.js';

export type * from './generated/types.js';
export { LensoApiError, type LensoClientOptions } from './generated/client.js';

export type LensoClient = {
  readonly identity: {
    createUser(
      input: Parameters<GeneratedLensoClient['createUser']>[0],
    ): ReturnType<GeneratedLensoClient['createUser']>;
  };
};

export function createClient(options: LensoClientOptions): LensoClient {
  const generated = new GeneratedLensoClient(options);

  return {
    identity: {
      createUser: (input) => generated.createUser(input),
    },
  };
}
