import { afterEach, expect, test } from "bun:test";
import { mkdir, mkdtemp, realpath, rm, stat, symlink } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import {
  createDevSupervisor,
  createEngineSession,
  diagnostic,
  generate,
  type DevSupervisorEvent,
} from "@lenso/engine";
import { validateContract } from "@lenso/runtime-sdk/schema";

const roots: string[] = [];
afterEach(async () => {
  for (const root of roots.splice(0)) await rm(root, { recursive: true, force: true });
});
const workspace = resolve(import.meta.dir, "../../..");
const authoring = resolve(import.meta.dir, "../src/authoring.ts");
const runtimeEngine = resolve(import.meta.dir, "../src/engine.ts");
const runtimeIndex = resolve(import.meta.dir, "../src/index.ts");
const helper = (length: number) =>
  `export const input = { type: 'string', maxLength: ${length} } as const;`;

async function fixture(source?: string) {
  const root = await mkdtemp(join(tmpdir(), "lenso-runtime-authoring-"));
  roots.push(root);
  await mkdir(join(root, "node_modules"));
  await symlink(
    join(workspace, "packages/runtime/node_modules/@lenso"),
    join(root, "node_modules/@lenso"),
  );
  await Bun.write(join(root, "src/schema.ts"), helper(5));
  await Bun.write(
    join(root, "src/contracts.ts"),
    source ??
      `
    import { runtimeSchema, defineRuntimeService } from ${JSON.stringify(authoring)};
    import { input } from './schema';
    export const service = defineRuntimeService({ version: '1', methods: {
      greet: { mode: 'unary', input: runtimeSchema(input), output: runtimeSchema({type:'string'}), error: runtimeSchema({type:'null'}) },
    }});
    export const v1Contract = service.v1Contract;
  `,
  );
  await Bun.write(
    join(root, "lenso.config.ts"),
    `
    import { definePlugin } from '@lenso/core';
    import { createRuntimePlugin } from ${JSON.stringify(runtimeIndex)};
    import { service, v1Contract } from './src/contracts';
    const runtime = createRuntimePlugin({ id:'runtime', connection: {namespace:'test',service:'static',contract:v1Contract},
      clients: () => { throw Error('Generation must not connect'); },
      exports: service.bind(() => { throw Error('Generation must not bind'); }),
    });
    const native = definePlugin({ id:'native', setup() { throw Error('Generation must not run setup'); } });
    export default { plugins: [runtime, native] };
  `,
  );
  await Bun.write(
    join(root, "lenso.engine.ts"),
    `
    import { runtimeContractPlugin } from ${JSON.stringify(runtimeEngine)};
    export default { plugins: [runtimeContractPlugin({source:'src/contracts.ts',watch:['dynamic']})] };
  `,
  );
  await mkdir(join(root, "dynamic"));
  return root;
}

test("Engine generates exact v1 JSON without setup and owns its tracked source graph", async () => {
  const root = await fixture();
  const engine = createEngineSession(root, "generate");
  try {
    await engine.prepare();
    expect(engine.session.invalidationInputs()).toContain(
      await realpath(join(root, "src/contracts.ts")),
    );
    expect(engine.session.invalidationInputs()).toContain(
      await realpath(join(root, "src/schema.ts")),
    );
    expect(engine.session.invalidationInputs()).toContain(await realpath(join(root, "dynamic")));
    await engine.session.generate();
  } finally {
    await engine.session.close();
  }
  const file = join(root, ".lenso/runtime/v1.contract.json");
  const expected = (await import(pathToFileURL(join(root, "src/contracts.ts")).href)).v1Contract;
  const bytes = await Bun.file(file).text();
  expect(JSON.parse(bytes)).toEqual(expected);
  validateContract(JSON.parse(bytes));
  const before = (await stat(file)).mtimeMs;
  await generate(root);
  expect(await Bun.file(file).text()).toBe(bytes);
  expect((await stat(file)).mtimeMs).toBe(before);
  // Raw SDK contracts remain live even within an already imported module.
  expected.methods.greet.input.maxLength = 9;
  await generate(root);
  expect((await Bun.file(file).json()).methods.greet.input.maxLength).toBe(9);
  await Bun.write(file, "user changes");
  const detail = diagnostic(await generate(root).catch((error: unknown) => error));
  expect(detail.code).toBe("generated-file-modified");
  expect(await Bun.file(file).text()).toBe("user changes");
});

function includes(
  detail: ReturnType<typeof diagnostic>,
  predicate: (value: ReturnType<typeof diagnostic>) => boolean,
): boolean {
  return predicate(detail) || !!detail.causes?.some((cause) => includes(cause, predicate));
}

test("Engine retains a known unsupported schema path before app assembly imports it", async () => {
  const root = await fixture(`
    import {runtimeSchema} from ${JSON.stringify(authoring)};
    runtimeSchema({type:'object',properties:{name:{type:'string',pattern:'^x$'}},required:['name'],additionalProperties:false});
    export const v1Contract = {};
  `);
  const detail = diagnostic(await generate(root).catch((error: unknown) => error));
  expect(
    includes(
      detail,
      (value) =>
        value.code === "invalid-runtime-contract" &&
        value.message.includes("#/properties/name/pattern") &&
        value.source?.file === "src/contracts.ts",
    ),
  ).toBe(true);
  expect(await Bun.file(join(root, ".lenso/runtime/v1.contract.json")).exists()).toBe(false);
});

test("raw invalid contract exports identify method schema paths; arbitrary import failures stay opaque", async () => {
  const root = await fixture(
    `export const v1Contract = {version:'1',methods:{greet:{mode:'unary',input:{type:'string',pattern:'x'},output:{type:'string'},error:{type:'null'}}}};`,
  );
  const detail = diagnostic(await generate(root).catch((error: unknown) => error));
  expect(includes(detail, (value) => value.message.includes("#/methods/greet/input/pattern"))).toBe(
    true,
  );
  const bad = await fixture(`throw Error('private-token=not-a-schema-error');`);
  expect(
    JSON.stringify(diagnostic(await generate(bad).catch((error: unknown) => error))),
  ).not.toContain("private-token");
});

test("existing Engine dev watcher regenerates an imported schema and closes its owned processes", async () => {
  const root = await fixture();
  await Bun.write(
    join(root, "src/server.ts"),
    `
    import {reportDevReady} from '@lenso/engine/dev-ready';
    const timer = setInterval(() => {}, 1000);
    process.on('SIGTERM', () => { clearInterval(timer); process.exit(0); });
    reportDevReady({ capabilities: ['runtime-contract'] });
  `,
  );
  const events: DevSupervisorEvent[] = [];
  const dev = await createDevSupervisor({
    root,
    stdout: "ignore",
    stderr: "ignore",
    stopTimeoutMs: 2000,
    onEvent(event) {
      events.push(event);
    },
  });
  const untilReady = async (count: number) => {
    const end = performance.now() + 10_000;
    while (events.filter((event) => event.type === "ready").length < count) {
      if (performance.now() > end)
        throw new Error(`Engine did not become ready: ${JSON.stringify(events)}`);
      await Bun.sleep(25);
    }
  };
  try {
    await untilReady(1);
    expect(
      (await Bun.file(join(root, ".lenso/runtime/v1.contract.json")).json()).methods.greet.input
        .maxLength,
    ).toBe(5);
    await Bun.write(join(root, "src/schema.ts"), helper(12));
    await untilReady(2);
    expect(
      (await Bun.file(join(root, ".lenso/runtime/v1.contract.json")).json()).methods.greet.input
        .maxLength,
    ).toBe(12);
    expect(events.filter((event) => event.type === "failed")).toEqual([]);
  } finally {
    await dev.close();
  }
}, 20_000);
