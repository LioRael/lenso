# @lenso/console

A headless client and compiled page-extension registry for Lenso Console hosts. This package contains no frontend application or UI framework.

```ts
import { ConsoleClient, ConsolePages } from "@lenso/console";

const client = new ConsoleClient("http://127.0.0.1:3000/console", () => sessionToken);
const manifest = await client.manifest();
const pages = new ConsolePages(client);
pages.register({ id: "files", pages: {
  files: context => createFilesView(context),
} });
const files = manifest.pages.find(page => page.id === "files");
if (files) {
  const rows = await client.call(files.list);
  const view = pages.render(files);
}
```

The host supplies `createFilesView`; its result can be a native React element or another framework's view value. Page factories are compiled, locally registered code, never executable code from a remote manifest. Duplicate extensions and page ids are rejected before installation.

The server authorizes every request. Network manifests and error envelopes are validated; each extension must validate its action's `unknown` result. The client rejects redirects to avoid forwarding credentials to another endpoint. An AbortSignal may be supplied per request. Node.js execution is covered locally; the Team Desk host additionally exercises real Chromium and Playwright WebKit workflows against both native backends. Actual Safari/iPhone and other browser/platform support still need their own verification.

Use `client.callParsed(action, resultParser, input, signal)` when a view knows its result contract. For example, `const rows = await client.callParsed(files.list, z.array(fileSchema))` infers the validated result. The parser is structural; Zod is optional for action results. Server errors retain their codes, while malformed bodies, invalid manifests and result-parser failures are safe `internal` errors with private causes. Input validation is not part of this method, and a failed result check does not imply the server rolled back a write. Raw `call` remains available for unknown contracts. No automatic retry is performed.

The base URL's pathname is normalized independently of its query. Configured query parameters are retained on manifest and action requests; fragments are not transmitted. Cancellation is checked around credential acquisition and response reading, preserving the original reason and preventing late dispatch. An arbitrary asynchronous token provider still owns its own completion/cancellation work.
