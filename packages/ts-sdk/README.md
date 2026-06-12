# @lenso/ts-sdk

Minimal TypeScript SDK generated from `contracts/openapi/app-api.v1.yaml`.

```ts
import { createClient, LensoApiError } from '@lenso/ts-sdk';

const client = createClient({
  baseUrl: 'http://localhost:3000',
  headers: {
    'x-correlation-id': 'corr_example',
  },
});

try {
  const user = await client.identity.createUser({
    email: 'ada@example.com',
    display_name: 'Ada',
  });

  console.log(user.id);
} catch (error) {
  if (error instanceof LensoApiError) {
    console.error(error.response.error.code);
  }

  throw error;
}
```

## Scripts

- `pnpm generate`: regenerate SDK files from the committed OpenAPI contract.
- `pnpm typecheck`: typecheck the SDK.
- `pnpm build`: emit JavaScript and declarations into `dist/`.
