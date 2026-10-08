# Development output

`lenso dev --root <project>` watches the entry and restarts a fresh Bun process.
The human startup view uses stderr. TTY output has modest colors; `NO_COLOR`,
CI, non-TTY and `TERM=dumb` output stays plain.

Readiness is explicit. In the development entry, after application startup and
listener binding succeed, report actual runtime information with the public helper:

```ts
import { reportDevReady } from "lenso-cli/dev";

reportDevReady({
  urls: [server.url.href],
  capabilities: ["web"],
});
```

The included greeting entry already calls this helper. It is a no-op outside supervised dev; authors never need to handwrite IPC messages. Custom entries call it after successful startup. Service-only entries can omit `urls`. Use enabled capability names, without
configuration values or secrets. Entries without this signal remain Starting;
spawning a process alone does not prove readiness. Failed starts keep watching
for source changes. The displayed URL is the reported listener origin, with no
credentials, query, path or guessed network interfaces.

`createDevPresentation` in `src/dev-presentation.ts` accepts a project root and
optional `mode: "json"`. JSON mode emits no presentation output; the CLI's
protocol layer owns its results and diagnostics. The module does not wrap
console or intercept application logs.
