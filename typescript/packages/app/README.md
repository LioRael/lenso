# @lenso/app

Native TypeScript application primitives for Lenso. No Rust host or CLI process is required.

```ts
import { z } from "zod";
import { Executor, defineAction, emptyInput, requireOwner } from "@lenso/app";

const action = defineAction<{ owner: string; bytes: Uint8Array }>();
const read = action({
  id: "files.read",
  input: emptyInput,
  output: z.instanceof(Uint8Array),
  authorize(context) { requireOwner(context.principal, context.resources.owner); },
  run(context) { return context.resources.bytes; },
});

const executor = new Executor(record => { logger.info(record); });
const bytes = await executor.call(read, context, undefined);
```

Schemas can be Zod or any synchronous `parse(unknown)` object; Zod is not a dependency of the core package. `callExternal(read, context, value)` uses the same input schema automatically. Supply a codec only when the wire representation differs from the native value.

Here `context.resources` contains a file selected by trusted application code, including its stored owner. For an id-based lookup, load the file's owner from storage; a caller-supplied owner is not proof of ownership.

Native calls preserve native types. Declared parsers can validate and normalize those native values at both entrypoints; without a parser, values pass through unchanged. Use `allOf` / `anyOf` to compose policies, `defineResource` / `withResources` for configured native dependencies, and `DurableEvent` for persistent subscriptions. Subpath exports are `action`, `access`, `resource`, `module`, `task`, `events` and `console`.

Optional `validateInput` and `validateOutput` invariant hooks may be synchronous or asynchronous and are awaited in pipeline order. Parsers remain synchronous.

Record completion is awaited; the observer must not throw or reject. Cancellation is cooperative. Use native transactions or an outbox for effects spanning operations. See the repository semantics and runnable private-files application for full setup.

Console page definitions accept native Actions directly: `{ id: "files", title: "Files", list, detail, actions: [remove] }`. Use `consoleOperation(action, codec)` when a custom wire representation is required. Reusing the same Action across pages reuses its binding; distinct handlers with the same id are rejected. Heterogeneous modules can be registered in an ordinary loop without erasing their concrete exports.

Operations that are not page controls can be passed as the third `ConsoleExtension` constructor argument, e.g. `new ConsoleExtension(executor, pages, [session, drain])`. They use the same bindings and must appear in the catalog, even when not referenced by a page. This does not expose arbitrary catalog actions automatically.

`validateConsoleManifest(description)` checks typed protocol invariants (module/operation uniqueness and page references) after a transport has checked the JSON shape. The Console client and native manifest builders share these semantics. It is not a decoder for unknown input.
