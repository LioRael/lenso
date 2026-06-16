import { GeneratedLensoClient, type LensoClientOptions } from './generated/client.js';

export type * from './generated/types.js';
export { LensoApiError, type LensoClientOptions } from './generated/client.js';

export type LensoClient = {
  readonly auth: {
    readonly password: {
      login(
        input: Parameters<GeneratedLensoClient['authPasswordLogin']>[0],
      ): ReturnType<GeneratedLensoClient['authPasswordLogin']>;
      register(
        input: Parameters<GeneratedLensoClient['authPasswordRegister']>[0],
      ): ReturnType<GeneratedLensoClient['authPasswordRegister']>;
    };
  };
  readonly identity: {
    createUser(
      input: Parameters<GeneratedLensoClient['createUser']>[0],
    ): ReturnType<GeneratedLensoClient['createUser']>;
  };
};

export function createClient(options: LensoClientOptions): LensoClient {
  const generated = new GeneratedLensoClient(options);

  return {
    auth: {
      password: {
        login: (input) => generated.authPasswordLogin(input),
        register: (input) => generated.authPasswordRegister(input),
      },
    },
    identity: {
      createUser: (input) => generated.createUser(input),
    },
  };
}
