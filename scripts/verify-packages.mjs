import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";

const root = resolve(import.meta.dirname, "..");
const run = promisify(execFile);
const directory = await mkdtemp(join(tmpdir(), "lenso-packages-"));
try {
  const archives = [];
  for (const name of ["app", "sqlite", "console"]) {
    const { stdout } = await run("npm", ["pack", "--json", "--pack-destination", directory], {
      cwd: join(root, "typescript/packages", name),
    });
    const [{ filename, files }] = JSON.parse(stdout);
    assert.ok(files.some((file) => file.path === "dist/index.d.ts"));
    assert.ok(files.some((file) => file.path === "dist/index.js"));
    archives.push(join(directory, filename));
  }
  const dependency = await run("npm", ["pack", join(root, "node_modules/zod"), "--ignore-scripts", "--json", "--pack-destination", directory], { cwd: directory });
  archives.push(join(directory, JSON.parse(dependency.stdout)[0].filename));
  await writeFile(join(directory, "package.json"), JSON.stringify({ private: true, type: "module" }));
  await run("npm", ["install", "--offline", "--ignore-scripts", "--no-audit", "--no-fund", ...archives], { cwd: directory });
  const consumer = `
import { Executor, type Action, type Context, type Task, Module, Catalog, defineAction, defineResource, withResources } from '@lenso/app';
import { requireScope, allOf } from '@lenso/app/access';
import { withResource } from '@lenso/app/resource';
import { Event } from '@lenso/app/events';
import { consoleManifest, ConsoleExtension, consoleOperation } from '@lenso/app/console';
import type { TaskCodec } from '@lenso/app/task';
import { SqliteQueue } from '@lenso/sqlite';
import { ConsoleClient } from '@lenso/console';
const action: Action<{ value: number }, number, { offset: number }> = {
  id: 'consumer.read', authorize(context) { requireScope(context.principal, 'read'); },
  run(context, input) { return input.value + context.resources.offset; }
};
const context: Context<{ offset: number }> = {
  callId: 'consumer', resources: { offset: 1 },
  principal: { subject: 'alice', scopes: ['read'] }, signal: new AbortController().signal
};
const executor = new Executor(() => {});
const number = { parse(value: unknown) { if (typeof value !== 'number') throw new TypeError('number required'); return value; } };
const define = defineAction<{ offset: number }>();
const inferred = define({ id: 'inferred', input: number, output: number,
  authorize: allOf(context => requireScope(context.principal, 'read'), (context, input) => {
    if (context.resources.offset < 0 || input < 0) throw new Error('Invalid policy fixture');
  }),
  run: (context, input) => context.resources.offset + input });
const inferredValue: number = await executor.call(inferred, context, 2);
if (inferredValue !== 3) throw new Error('Inferred action failed');
const nativeResource = defineResource({ id: 'native', config: number, open: value => new Map([['native', value]]), close() {} });
await withResources({ db: nativeResource.configure(7) }, ({ db }) => { const value: number | undefined = db.get('native'); return value; });
const value: number = await executor.call(action, context, { value: 2 });
if (value !== 3) throw new Error('Invalid native result');
if (false) {
  // @ts-expect-error Inputs remain inferred from their parser without manual DTO annotations.
  await executor.call(inferred, context, 'wrong');
  // @ts-expect-error A codec cannot widen the native input contract.
  await executor.callExternal(inferred, context, null, { decode: () => 'wrong', encode: value => value });
  // @ts-expect-error Output schema rejects a handler returning the wrong type.
  define({ id: 'bad-output', input: number, output: number, authorize() {}, run: () => 'wrong' });
  // @ts-expect-error A module requires an actual Action, not a descriptor with only an id.
  new Module('fake', { actions: { fake: { id: 'fake' } } });
  // @ts-expect-error The public declaration must reject the wrong input type.
  await executor.call(action, context, { value: 'wrong' });
  // @ts-expect-error The resource contract must remain checked.
  await executor.call(action, { ...context, resources: {} }, { value: 2 });
}
const codec: TaskCodec<string> = { encode: value => value, decode: text => text };
const task: Task<string, string, {}> = { id: 'consumer.task', input: codec, output: codec, run: (_, value) => value };
const heterogeneous = [new Module('number', { actions: { inferred } }), new Module('object', { actions: { action } })];
const combined = new Catalog();
for (const module of heterogeneous) combined.register(module);
const extension = new ConsoleExtension(executor, [{id:'direct', title:'Direct', list:inferred}]);
if (await extension.call(context, inferred.id, 2) !== 3) throw new Error('Direct Console binding failed');
if (false) {
  // @ts-expect-error Direct Console bindings retain their native resource requirements.
  await extension.call({...context, resources:{}}, inferred.id, 2);
  // @ts-expect-error A Console codec must preserve the Action input contract.
  consoleOperation(inferred, {decode: () => 'wrong', encode: value => value});
}
const catalog = new Catalog();
catalog.register(new Module('consumer', { actions: { action }, tasks: { task } }));
consoleManifest(catalog, [{ id: 'consumer', title: 'Consumer', list: action.id }]);
await withResource({ open: () => 1, close: () => {} }, value => value);
await new Event<bigint>('consumer.event').publish(1n);
const parsedClient = new ConsoleClient('https://example.test/console', () => 'token', async () => Response.json(3));
const parsedValue: number = await parsedClient.callParsed('number', number);
if (parsedValue !== 3) throw new Error('Parsed response contract failed');
if (false) {
  // @ts-expect-error The result type is inferred from its parser, not selected by the caller.
  const invalidResult: string = await parsedClient.callParsed('number', number);
}
if (typeof SqliteQueue !== 'function' || typeof ConsoleClient !== 'function') throw new Error('Missing package export');
`;
  await writeFile(join(directory, "consumer.ts"), consumer);
  await run(process.execPath, [join(root, "node_modules/typescript/bin/tsc"),
    "consumer.ts", "--strict", "--exactOptionalPropertyTypes", "--skipLibCheck", "false",
    "--target", "ES2024", "--module", "NodeNext", "--moduleResolution", "NodeNext",
    "--types", "node", "--typeRoots", join(root, "node_modules/@types"),
  ], { cwd: directory });
  await run(process.execPath, ["consumer.js"], { cwd: directory });
  const lock = JSON.parse(await readFile(join(directory, "package-lock.json"), "utf8"));
  for (const name of ["app", "sqlite", "console"]) {
    assert.ok(!lock.packages[`node_modules/@lenso/${name}`].link, "Consumer must use packed files rather than workspace links");
  }
  console.log("Packed app, sqlite and console: declarations, invalid calls and native Node imports passed.");
} catch (error) {
  if (error.stdout) console.error(error.stdout);
  if (error.stderr) console.error(error.stderr);
  throw error;
} finally { await rm(directory, { recursive: true, force: true }); }
