# @lenso/web-client

`@lenso/web-client` generates TypeScript types from a caller-selected OpenAPI
3.1 document and creates a small typed browser client. Use the App-selected
`lenso-openapi-plugin` output as that input. The generator does not claim that
an arbitrary local document is public or trusted. It does not scan
backend source, discover Capabilities, publish routes, or make an internal
Capability browser-accessible. The App-selected OpenAPI document is the entire
public surface.

```sh
lenso-web-client generate ./openapi.json ./src/generated/lenso-api.ts
```

```ts
import { createLensoWebClient, unwrap } from '@lenso/web-client';
import type { paths } from './generated/lenso-api';

const api = createLensoWebClient<paths>({
  baseUrl: 'http://127.0.0.1:3001',
  authentication: {
    kind: 'bearer',
    accessToken: () => sessionStorage.getItem('access-token') ?? undefined,
  },
});

const note = unwrap(await api.GET('/notes/{note_id}', {
  params: { path: { note_id: 'note-1' } },
}));
```

Session-cookie clients set `credentials: "include"` and must supply a CSRF
token callback for unsafe methods; a missing token rejects before `fetch`.
`unwrap` converts an RFC 9457-compatible
error body into `LensoApiError`; transport failures become
`LensoTransportError`. Pass an `AbortSignal` through the generated method
options. For a streaming response use `parseAs: "stream"` and
`unwrapStream`—the body remains incremental and is never silently buffered.

The support metadata marks ordinary request/response APIs and response body
streams as available. This package exposes no WebSocket or Workers Host API;
those need their target-specific integrations. Existing linked Rust Web
providers remain unchanged.
