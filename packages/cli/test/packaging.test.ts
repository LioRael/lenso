import { expect, test } from "bun:test";
import { mkdtemp, mkdir as createDirectory, realpath as canonicalPath, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join as joinPath, resolve } from "node:path";

const repository = resolve(import.meta.dir, "../../..");
const packages = [
  ["@lenso/core", "packages/lenso"],
  ["@lenso/engine", "packages/engine"],
  ["@lenso/cli", "packages/cli"],
  ["@lenso/manage", "packages/manage"],
  ["@lenso/auth", "packages/auth"],
  ["@lenso/mcp", "packages/mcp"],
  ["@lenso/web", "packages/web"],
  ["@lenso/workers", "packages/workers"],
  ["lenso-example-content-engine", "packages/cli/examples/content-plugin"],
  ["lenso-example-module-target", "packages/cli/examples/module-target-plugin"],
] as const;

async function run(cwd: string, args: string[]) {
  const env = { ...process.env };
  delete env.NODE_PATH;
  delete env.BUN_OPTIONS;
  const child = Bun.spawn([process.execPath, ...args], {
    cwd,
    env,
    stdout: "pipe",
    stderr: "pipe",
    stdin: "ignore",
  });
  const timeout = setTimeout(() => child.kill("SIGKILL"), 90000);
  try {
    const [code, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    if (code !== 0) throw new Error(`${args.join(" ")} exited ${code}\n${stdout}\n${stderr}`);
    return stdout;
  } finally {
    clearTimeout(timeout);
  }
}

// Build the framework packages in dependency order first.
test("packed Engine, CLI and external plugins work in a standalone consumer", async () => {
  const temporary = await canonicalPath(await mkdtemp(joinPath(tmpdir(), "lenso-packaging-")));
  try {
    const artifacts = joinPath(temporary, "tarballs");
    const consumer = joinPath(temporary, "consumer");
    await createDirectory(artifacts);
    await createDirectory(consumer);
    expect(
      (await canonicalPath(consumer)).startsWith((await canonicalPath(repository)) + "/"),
    ).toBe(false);
    const dependencies: Record<string, string> = {};
    for (const [name, directory] of packages) {
      const tarball = joinPath(artifacts, `${name.replaceAll("/", "-").replaceAll("@", "")}.tgz`);
      await run(joinPath(repository, directory), [
        "pm",
        "pack",
        "--ignore-scripts",
        "--filename",
        tarball,
      ]);
      dependencies[name] = `file:${tarball}`;
    }
    const engineOnly = joinPath(temporary, "engine-only");
    await createDirectory(engineOnly);
    await Bun.write(
      joinPath(engineOnly, "package.json"),
      JSON.stringify({
        private: true,
        type: "module",
        dependencies: {
          "@lenso/core": dependencies["@lenso/core"],
          "@lenso/engine": dependencies["@lenso/engine"],
          "@lenso/manage": dependencies["@lenso/manage"],
        },
        overrides: {
          "@lenso/core": dependencies["@lenso/core"],
          "@lenso/engine": dependencies["@lenso/engine"],
          "@lenso/manage": dependencies["@lenso/manage"],
        },
      }),
    );
    await run(engineOnly, ["install", "--ignore-scripts"]);
    await run(engineOnly, [
      "-e",
      `
      import assert from 'node:assert/strict';
      import {generate,startEngineDevCycle} from '@lenso/engine';
      import {defineManage,createManageAdapter} from '@lenso/manage';
      import {createAgentTools} from '@lenso/manage/agent';
      assert.equal(typeof defineManage,'function');
      assert.equal(typeof createManageAdapter,'function');
      assert.equal(typeof createAgentTools,'function');
      assert.throws(()=>Bun.resolveSync('@lenso/cli',process.cwd()));
      assert.throws(()=>Bun.resolveSync('@lenso/auth',process.cwd()));
      assert.throws(()=>Bun.resolveSync('@orpc/server',process.cwd()));
      await Bun.write('lenso.config.ts','export default {plugins:[]};');
      await Bun.write('lenso.engine.ts',\`export default {plugins:[{name:'standalone',setup(c){
        c.onCleanup(()=>Bun.write(c.root+'/closed','yes'));
      }}]};\`);
      assert.deepEqual(await generate(process.cwd()),[]);
      const cycle=await startEngineDevCycle(process.cwd());
      await cycle.ready();
      await cycle.close();
      assert.equal(await Bun.file('closed').text(),'yes');
    `,
    ]);
    await Bun.write(
      joinPath(consumer, "package.json"),
      JSON.stringify({
        name: "standalone-engine-consumer",
        private: true,
        type: "module",
        dependencies,
        overrides: Object.fromEntries(
          Object.entries(dependencies).filter(([name]) => name.startsWith("@lenso/")),
        ),
        devDependencies: {
          "@types/bun": "1.4.2",
          typescript: "7.0.2",
        },
      }),
    );
    await Bun.write(
      joinPath(consumer, "tsconfig.json"),
      JSON.stringify({
        compilerOptions: {
          target: "ES2022",
          module: "ESNext",
          moduleResolution: "Bundler",
          strict: true,
          noEmit: true,
          skipLibCheck: true,
          types: ["bun"],
        },
        include: ["api-types.ts"],
      }),
    );
    await run(consumer, ["install", "--ignore-scripts"]);
    for (const [name] of packages) {
      const installed = joinPath(consumer, "node_modules", name);
      expect(await canonicalPath(installed)).toBe(installed);
      const manifest = await Bun.file(joinPath(installed, "package.json")).json();
      expect(manifest.name).toBe(name);
      for (const group of ["dependencies", "peerDependencies"]) {
        for (const version of Object.values(manifest[group] ?? {})) {
          expect(String(version)).not.toMatch(/workspace:|link:|file:/);
        }
      }
    }
    await Bun.write(
      joinPath(consumer, "api-types.ts"),
      `
      import {build,discover,generate,createDevSupervisor,EngineError} from '@lenso/engine';
      import {defineEnginePlugin,defineEngineConfig} from '@lenso/engine/authoring';
      import {definePlugin,bindConfig,definePluginConfig,startApp} from '@lenso/core';
      import {defineOperation,type Operation} from '@lenso/engine/operations';
      import {invoke} from '@lenso/cli';
      import {defineManage,selectManageOperations,bindManageOperation} from '@lenso/manage';
      import {createManageRouter} from '@lenso/manage/orpc';
      const managementInput={"~standard":{version:1 as const,vendor:"consumer",validate:(value:unknown)=>({value})}};
      const managed=definePlugin({id:'managed',setup:()=>({
        read:(_input:unknown,context:{evidence:string})=>context.evidence,
      })});
      const operation=defineOperation({
        plugin:managed,method:'read',input:managementInput,context:true,description:'Read',
      });
      operation satisfies Operation<{evidence:string}>;
      bindManageOperation(operation,{context:{evidence:'launch'}});
      invoke({plugins:[managed],operations:[operation]},'managed','read',{},()=>({context:{evidence:'launch'}}));
      // @ts-expect-error The actual CLI invoke binding retains the service context type.
      invoke({plugins:[managed],operations:[operation]},'managed','read',{},()=>({context:{evidence:42}}));
      // @ts-expect-error Context must match the actual service's second parameter.
      bindManageOperation(operation,{context:{evidence:42}});
      // @ts-expect-error Required context cannot be silently omitted from the declaration.
      defineOperation({plugin:managed,method:'read',input:managementInput,description:'Read'});
      const manage=defineManage({plugin:managed,operations:[operation]});
      selectManageOperations(manage,['read']);
      // @ts-expect-error Entry selection cannot invent a service method.
      selectManageOperations(manage,['hidden']);
      export const routerFactory=createManageRouter;
      import {createBunListenerPlugin} from '@lenso/web/bun';
      const web=definePlugin({id:'typed-web',setup:()=>({fetch:async()=>new Response('ok')})});
      export const listener=createBunListenerPlugin({
        web,hostname:'127.0.0.1',port:0,ingress:()=>undefined,
      });
      // @ts-expect-error The application must choose its ingress policy.
      createBunListenerPlugin({web,hostname:'127.0.0.1',port:0});
      import type {StandardSchemaV1} from '@standard-schema/spec';
      const input:StandardSchemaV1<{port?:string},{port:number}>={
        '~standard':{version:1,vendor:'test',validate:()=>({value:{port:3001}})}
      };
      const contract=definePluginConfig({schema:input});
      const instance=bindConfig(contract,{port:'3001'},{id:'typed-config',setup(_c,config){
        config.port satisfies number;
        // @ts-expect-error Schema output, not schema input, reaches setup.
        config.port satisfies string;
        return config;
      }});
      // @ts-expect-error Plain values must match schema input, not output.
      bindConfig(contract,{port:3001},{id:'bad-input',setup:(_c,config)=>config});
      export async function useConfig(){
        const app=await startApp({plugins:[instance]});
        app.get(instance).port satisfies number;
        await app.stop();
      }
      export const config=defineEngineConfig({target:'custom',plugins:[
        defineEnginePlugin({name:'typed/implicit',setup:c=>c.generate('implicit',()=>[])}),
        defineEnginePlugin({name:'typed/async-implicit',setup:async c=>c.watch('source.ts')}),
        defineEnginePlugin({name:'typed/consumer',setup(c){
          c.discover('input',async()=>['source.ts']);
          c.generate('output',s=>[{path:'output.ts',content:JSON.stringify(s.sources)}]);
          c.target('custom',b=>b.bundle({entry:b.entry,platform:'browser'}));
          c.dev('lifecycle',(_event,s)=>{c.watch(s.convention.config)});
          c.dev('implicit-lifecycle',()=>c.watch('source.ts'));
          c.onCleanup(async()=>{});
          // @ts-expect-error Build targets are not terminal rendering callbacks.
          c.target('invalid',()=>42);
        }})
      ]});
      export async function useEngine(root:string){
        try{
          const discovery=await discover(root);
          const manifest=await generate(root);
          const directory:string=await build(root);
          const dev=await createDevSupervisor({root,onEvent(event){
            if(event.type==='failed') event.diagnostic.code satisfies string;
          }});
          await dev.close();
          return {discovery,manifest,directory};
        }catch(error){
          if(error instanceof EngineError) return error.diagnostic;
          throw error;
        }
      }
    `,
    );
    await run(consumer, ["run", "tsc", "-p", "tsconfig.json"]);
    await Bun.write(joinPath(consumer, "verify.ts"), `await (${verifyConsumer.toString()})();`);
    expect(await run(consumer, ["verify.ts"])).toContain("packaged consumer verified");
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}, 180000);

async function verifyConsumer() {
  const assert: typeof import("node:assert/strict") = (await import("node:assert/strict")).default;
  const { mkdir, realpath, stat } = await import("node:fs/promises");
  const { dirname, join } = await import("node:path");
  const { pathToFileURL } = await import("node:url");
  const {
    discover,
    generate,
    build,
    createEngineSession,
    EngineError,
    diagnostic,
    startEngineDevCycle,
    createDevSupervisor,
  } = await import("@lenso/engine");
  const { EngineError: diagnosticsError, diagnostic: subpathDiagnostic } =
    await import("@lenso/engine/diagnostics");

  assert.equal(typeof createEngineSession, "function");
  assert.equal(typeof startEngineDevCycle, "function");
  assert.equal(typeof EngineError, "function");
  assert.equal(typeof diagnostic, "function");
  assert.equal(typeof createDevSupervisor, "function");
  assert.equal(EngineError, diagnosticsError);
  assert.equal(diagnostic, subpathDiagnostic);
  assert.equal(typeof (await import("@lenso/engine")).generate, "function");
  assert.equal(typeof (await import("@lenso/engine/dev-ready")).reportDevReady, "function");
  const core = await import("@lenso/core");
  const configuration = await import("@lenso/core/config");
  assert.equal(core.ConfigError, configuration.ConfigError);
  assert.equal(core.ConfigSourceError, configuration.ConfigSourceError);
  assert.equal(typeof (await import("@lenso/core/config/env")).envSource, "function");
  assert.equal(typeof (await import("@lenso/core/config/file")).jsonFileSource, "function");
  const cliPackage = await import("@lenso/cli");
  const { definePlugin: createManagedPlugin, startApp: startManagedApp } =
    await import("@lenso/core");
  const { defineOperation } = await import("@lenso/engine/operations");
  const { defineManage, describeManage, selectManageOperations, createManageAdapter } =
    await import("@lenso/manage");
  const { createAgentTools } = await import("@lenso/manage/agent");
  const { createManageRouter } = await import("@lenso/manage/orpc");
  const { bearerEvidence } = await import("@lenso/auth/fetch");
  const { call: callProcedure } = await import("@orpc/server");
  const mcpSpecifier = "@lenso/mcp";
  assert.equal(typeof (await import(mcpSpecifier)).serveStdio, "function");
  let managedSetup = 0;
  let managedCleanup = 0;
  const managed = createManagedPlugin({
    id: "packaged-managed",
    setup(context) {
      managedSetup++;
      context.onCleanup(() => {
        managedCleanup++;
      });
      return {
        read: (_input: unknown, binding: { evidence: string | null }) => binding.evidence,
        hidden: () => "not exposed",
      };
    },
  });
  const managedInput = {
    "~standard": {
      version: 1 as const,
      vendor: "consumer",
      validate: (value: unknown) => ({ value }),
      jsonSchema: { input: () => ({ type: "object" }), output: () => ({}) },
    },
  };
  const managedOperation = defineOperation({
    plugin: managed,
    method: "read",
    input: managedInput,
    context: true,
    description: "Read",
  });
  const manage = defineManage({ plugin: managed, operations: [managedOperation] });
  assert.equal(describeManage(manage).operations.length, 1);
  assert.equal(managedSetup, 0);
  const selected = selectManageOperations(manage, ["read"]);
  const runningManaged = await startManagedApp({ plugins: [managed] });
  try {
    const adapter = createManageAdapter({
      running: runningManaged,
      plugins: [managed],
      operations: selected,
      binding: () => ({ context: { evidence: "agent-launch" } }),
      canList: () => true,
    });
    const tools = await createAgentTools(adapter);
    assert.equal(await tools[0]!.invoke({}), "agent-launch");
    await assert.rejects(adapter.invoke(managed.id, "hidden", {}));
    const router = createManageRouter({
      running: runningManaged,
      plugins: [managed],
      operations: selected,
      evidence: bearerEvidence,
      canList: () => true,
      binding: (_operation, _input, evidence) => ({ context: { evidence: evidence.evidence } }),
    });
    assert.equal(
      await callProcedure(
        router.invoke,
        {
          pluginId: managed.id,
          method: "read",
          input: {},
        },
        {
          context: {
            request: new Request("https://consumer.invalid/rpc", {
              headers: { authorization: "Bearer current-request" },
            }),
          },
        },
      ),
      "current-request",
    );
    assert.equal(managedSetup, 1);
    assert.equal(managedCleanup, 0);
  } finally {
    await runningManaged.stop();
  }
  assert.equal(
    await cliPackage.invoke(
      { plugins: [managed], operations: selected },
      managed.id,
      "read",
      {},
      () => ({ context: { evidence: "cli-launch" } }),
    ),
    "cli-launch",
  );
  assert.equal(managedSetup, 2);
  assert.equal(managedCleanup, 2);
  for (const name of [
    "discover",
    "generate",
    "build",
    "createEngineSession",
    "startEngineDevCycle",
    "reportDevReady",
  ])
    assert.equal(name in cliPackage, false, `@lenso/cli must not export ${name}`);
  for (const specifier of ["@lenso/cli/dev", "@lenso/cli/engine"])
    assert.throws(() => Bun.resolveSync(specifier, process.cwd()));
  for (const [dependency, importer] of [
    ["@lenso/core", "@lenso/engine"],
    ["@lenso/core", "@lenso/workers"],
    ["@lenso/engine", "@lenso/cli"],
    ["@lenso/engine/authoring", "lenso-example-content-engine"],
    ["@lenso/engine/authoring", "lenso-example-module-target"],
  ]) {
    const origin = Bun.resolveSync(importer!, process.cwd());
    assert.equal(
      Bun.resolveSync(dependency!, dirname(origin)),
      Bun.resolveSync(dependency!, process.cwd()),
    );
  }
  for (const specifier of [
    "@lenso/core",
    "@lenso/core/plugin",
    "@lenso/core/config",
    "@lenso/core/config/env",
    "@lenso/core/config/file",
    "@lenso/engine",
    "@lenso/engine/authoring",
    "@lenso/engine/dev-ready",
    "@lenso/engine/diagnostics",
    "@lenso/cli",
    "lenso-example-content-engine",
    "lenso-example-module-target",
    "@lenso/workers",
  ]) {
    const resolved = Bun.resolveSync(specifier, process.cwd());
    assert.ok(resolved.startsWith(process.cwd() + "/node_modules/"), resolved);
    assert.equal(await realpath(resolved), resolved);
  }

  async function fixture(name: string, config = "export default {plugins:[]};") {
    const root = join(process.cwd(), name);
    await mkdir(root);
    await Bun.write(join(root, "lenso.config.ts"), config);
    return root;
  }
  const configured = await fixture(
    "configured",
    String.raw`
  import {bindConfig,defineApp,definePluginConfig,valuesSource} from '@lenso/core';
  import {envSource} from '@lenso/core/config/env';
  import {jsonFileSource} from '@lenso/core/config/file';
  import {defineOperation} from '@lenso/cli';
  const schema={'~standard':{version:1,vendor:'test',validate(value){
    if(typeof value.enabled!=='boolean') return {issues:[{path:['enabled'],message:'raw-private-error'}]};
    return {value:{enabled:value.enabled,label:value.label+':validated'}};
  }}};
  const instance=bindConfig(definePluginConfig({schema}),[
    valuesSource({enabled:true,label:'values'}),
    jsonFileSource({id:'file',root:import.meta.dir,path:'config.json'}),
    envSource({id:'env',read:()=> 'false',bindings:{enabled:{name:'ONLY_BOUND_KEY',type:'boolean'}}})
  ],{id:'configured',setup(_context,config){return {read:()=>config}}});
  export const operations=[defineOperation({plugin:instance,method:'read',description:'Read test config',
    input:{'~standard':{version:1,vendor:'test',validate:value=>({value})}}})];
  export default defineApp({plugins:[instance]});
`,
  );
  // Missing file is harmless for inspection, but fatal for startup.
  const inspectedConfig = await cliPackage.inspect(configured);
  assert.equal(inspectedConfig.plugins[0]?.config?.sources.length, 3);
  await assert.rejects(cliPackage.call(configured, "configured", "read", {}), (error) => {
    const detail = diagnostic(error);
    assert.equal(detail.phase, "config");
    assert.equal(detail.causes?.[0]?.code, "config-file-missing");
    assert.equal(detail.causes?.[0]?.pluginId, "configured");
    assert.deepEqual(detail.causes?.[0]?.details, { sourceId: "file" });
    return true;
  });
  await Bun.write(join(configured, "config.json"), '{"enabled":true,"label":"file"}');
  assert.deepEqual(await cliPackage.call(configured, "configured", "read", {}), {
    enabled: false,
    label: "file:validated",
  });
  const defaults = await fixture(
    "defaults",
    String.raw`
  import {defineApp,definePlugin} from '@lenso/core';
  const dependency=definePlugin({id:'dependency',setup(){throw Error('no business setup during discovery')}});
  const dependent=definePlugin({id:'dependent',requires:[dependency],setup(){throw Error('no business setup during generation')}});
  export default defineApp({plugins:[dependent,dependency]});
`,
  );
  const invalid = await fixture(
    "invalid",
    "export default {plugins:[{id:'duplicate',setup(){}},{id:'duplicate',setup(){}}]};",
  );
  await assert.rejects(discover(invalid), (cause) => {
    assert.ok(cause instanceof EngineError);
    assert.ok(cause instanceof diagnosticsError);
    assert.equal(diagnostic(cause).code, "invalid-assembly");
    assert.equal(diagnostic(cause).causes?.[0]?.code, "duplicate-id");
    return true;
  });
  assert.deepEqual(
    (await discover(defaults)).ordered.map((p) => p.id),
    ["dependency", "dependent"],
  );
  await generate(defaults);
  const snapshots = await Promise.all(
    ["manifest.json", "server.ts", "client.ts"].map(async (file) => {
      const path = join(defaults, ".lenso", file);
      return { path, bytes: await Bun.file(path).text(), mtime: (await stat(path)).mtimeMs };
    }),
  );
  assert.ok(snapshots[0].bytes.includes('"schemaVersion": 1'));
  assert.ok(snapshots[1].bytes.includes("../lenso.config"));
  assert.ok(!snapshots[2].bytes.includes("startApp"));
  await Bun.sleep(25);
  await generate(defaults);
  for (const snapshot of snapshots) {
    assert.equal(await Bun.file(snapshot.path).text(), snapshot.bytes);
    assert.equal((await stat(snapshot.path)).mtimeMs, snapshot.mtime);
  }

  const external = await fixture("external");
  await mkdir(join(external, "content"));
  await Bun.write(join(external, "content/hello.md"), "Hello packaged plugin");
  await Bun.write(join(external, "assembly.ts"), "export default {plugins:[]};");
  await Bun.write(
    join(external, "browser.ts"),
    "import {documents} from './.lenso/content'; export const body=documents[0].body;",
  );
  await Bun.write(
    join(external, "lenso.engine.ts"),
    String.raw`
  import {defineEngineConfig,defineEnginePlugin} from '@lenso/engine/authoring';
  import {contentPlugin} from 'lenso-example-content-engine';
  import {moduleTarget} from 'lenso-example-module-target';
  export default defineEngineConfig({target:'browser-module',plugins:[
    contentPlugin(),
    defineEnginePlugin({name:'replacement',setup(c){
      c.convention(()=>({config:'assembly.ts',entry:'browser.ts'}),{replace:'lenso/defaults'});
      c.generate('client',()=>[{path:'client.ts',content:'export const replaced = true;'}],{replace:'lenso/defaults'});
    }}),
    moduleTarget({name:'browser-module',entry:'browser.ts'}),
  ]});
`,
  );
  await generate(external);
  assert.ok(
    (await Bun.file(join(external, ".lenso/content.ts")).text()).includes("Hello packaged plugin"),
  );
  assert.equal(
    await Bun.file(join(external, ".lenso/client.ts")).text(),
    "export const replaced = true;",
  );
  assert.ok((await Bun.file(join(external, ".lenso/server.ts")).text()).includes("assembly"));
  const browserOutput = await build(external);
  assert.equal(browserOutput, join(external, "dist/browser-module"));
  const browser = await import(pathToFileURL(join(browserOutput, "browser.js")).href);
  assert.equal(browser.body, "Hello packaged plugin");

  const dev = await fixture("dev");
  await mkdir(join(dev, "src"));
  await Bun.write(join(dev, "src/server.ts"), "export {};");
  const pluginFile = join(dev, "plugin.ts");
  await Bun.write(
    join(dev, "lenso.engine.ts"),
    "import {plugin} from './plugin'; export default {plugins:[plugin]};",
  );
  for (const value of ["first", "second"]) {
    const pluginSource = String.raw`
  import {appendFile} from 'node:fs/promises';
  import {defineEnginePlugin} from '@lenso/engine/authoring';
  export const plugin=defineEnginePlugin({name:'dev-resource',setup(c){
    c.onCleanup(()=>appendFile(c.root+'/events',${JSON.stringify("cleanup:" + value + "\n")}));
    c.generate('value',()=>[{path:'value.ts',content:${JSON.stringify(value)}}]);
    c.dev('lifecycle',event=>appendFile(c.root+'/events',event+':'+${JSON.stringify(value)}+'\n'));
  }});
`;
    await Bun.write(pluginFile, pluginSource);
    const cycle = await startEngineDevCycle(dev);
    try {
      assert.ok(cycle.watchFiles.includes(await realpath(pluginFile)));
      assert.ok(!cycle.watchFiles.some((path) => path.includes("/.lenso/")));
      assert.equal(await Bun.file(join(dev, ".lenso/value.ts")).text(), value);
      assert.ok(
        (await Bun.file(join(dev, "events")).text()).endsWith("beforeStart:" + value + "\n"),
      );
      await cycle.ready();
      assert.ok((await Bun.file(join(dev, "events")).text()).endsWith("ready:" + value + "\n"));
      assert.equal(cycle.close(), cycle.close());
    } finally {
      await cycle.close();
    }
  }
  assert.equal(
    await Bun.file(join(dev, "events")).text(),
    "beforeStart:first\nready:first\ncleanup:first\nbeforeStart:second\nready:second\ncleanup:second\n",
  );

  const finite = await fixture(
    "finite",
    String.raw`
  import {defineApp,definePlugin} from '@lenso/core';
  import {defineOperation} from '@lenso/cli';
  import {appendFile} from 'node:fs/promises';
  const p=definePlugin({id:'service',setup(c){
    c.onCleanup(()=>appendFile(import.meta.dir+'/runtime-events','cleanup\n'));
    return {run:input=>({value:input.value})};
  }});
  export const operations=[defineOperation({plugin:p,method:'run',description:'Run',
    input:{'~standard':{version:1,vendor:'fixture',validate:value=>({value})}}})];
  export default defineApp({plugins:[p]});
`,
  );
  await Bun.write(
    join(finite, "lenso.engine.ts"),
    "throw Error('finite commands must never import Engine config');",
  );
  async function cli(entry: string, args: string[], stdin?: string, expectedCode = 0) {
    const child = Bun.spawn([process.execPath, entry, ...args, "--root", finite, "--json"], {
      stdout: "pipe",
      stderr: "pipe",
      stdin: stdin === undefined ? "ignore" : "pipe",
    });
    if (stdin !== undefined && child.stdin && typeof child.stdin !== "number") {
      child.stdin.write(stdin);
      child.stdin.end();
    }
    const timeout = setTimeout(() => child.kill("SIGKILL"), 10000);
    try {
      const [code, out, err] = await Promise.all([
        child.exited,
        new Response(child.stdout).text(),
        new Response(child.stderr).text(),
      ]);
      assert.equal(code, expectedCode, err + "\n" + out);
      const envelope = JSON.parse(out);
      assert.equal(envelope.schemaVersion, 1);
      assert.equal(envelope.ok, expectedCode === 0);
      return envelope;
    } finally {
      clearTimeout(timeout);
    }
  }
  const installedBin = join(process.cwd(), "node_modules/@lenso/cli/dist/bin.js");
  const installedInspect = await cli(installedBin, ["inspect", "service", "run"]);
  assert.equal(installedInspect.data.operations[0].method, "run");
  assert.equal(await Bun.file(join(finite, "runtime-events")).exists(), false);
  assert.deepEqual(
    (await cli(installedBin, ["call", "service", "run", "--stdin"], '{"value":"installed"}')).data,
    { value: "installed" },
  );
  assert.equal(await Bun.file(join(finite, "runtime-events")).text(), "cleanup\n");
  const unknownOperation = await cli(
    installedBin,
    ["call", "service", "missing", "--stdin"],
    "{}",
    3,
  );
  assert.equal(unknownOperation.error.code, "unknown-operation");
  assert.equal(await Bun.file(join(finite, "runtime-events")).text(), "cleanup\n");
  const bundledBin = join(process.cwd(), "bundled-cli.js");
  const bundled = await Bun.build({
    entrypoints: [installedBin],
    outdir: process.cwd(),
    naming: "bundled-cli.js",
    target: "bun",
    packages: "bundle",
  });
  assert.equal(bundled.success, true, JSON.stringify(bundled.logs));
  assert.deepEqual(await cli(bundledBin, ["inspect", "service", "run"]), installedInspect);
  assert.deepEqual(
    (await cli(bundledBin, ["call", "service", "run", "--stdin"], '{"value":"bundled"}')).data,
    { value: "bundled" },
  );
  assert.deepEqual(
    await cli(bundledBin, ["call", "service", "missing", "--stdin"], "{}", 3),
    unknownOperation,
  );

  await Bun.write(
    join(dev, "src/server.ts"),
    String.raw`
  import {reportDevReady} from '@lenso/engine/dev-ready';
  import {appendFile} from 'node:fs/promises';
  const timer=setInterval(()=>{},1000);
  reportDevReady({capabilities:['packaged']});
  process.on('SIGTERM',async()=>{
    clearInterval(timer);
    await appendFile(import.meta.dir+'/../events','runtime-stop\n');
    process.disconnect?.();
  });
`,
  );
  const supervised = Bun.spawn([process.execPath, bundledBin, "dev", "--root", dev], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, NO_COLOR: "1" },
  });
  let presentation = "";
  const collect = async (stream: ReadableStream<Uint8Array>) => {
    for await (const chunk of stream) presentation += new TextDecoder().decode(chunk);
  };
  const collected = Promise.all([collect(supervised.stdout), collect(supervised.stderr)]);
  try {
    const deadline = Date.now() + 10000;
    while (!presentation.includes("Ready in")) {
      if (supervised.exitCode !== null || Date.now() > deadline) throw Error(presentation);
      await Bun.sleep(20);
    }
    assert.ok(presentation.includes("Enabled: packaged"));
    assert.ok(!presentation.includes("\x1b["));
  } finally {
    supervised.kill("SIGTERM");
    const forced = setTimeout(() => supervised.kill("SIGKILL"), 6000);
    try {
      await supervised.exited;
      await collected;
    } finally {
      clearTimeout(forced);
    }
  }
  assert.equal(supervised.exitCode, 0, presentation);
  assert.ok(
    (await Bun.file(join(dev, "events")).text()).endsWith(
      "beforeStart:second\nready:second\nruntime-stop\ncleanup:second\n",
    ),
  );

  const workers = await fixture("workers");
  await Bun.write(
    join(workers, "worker.ts"),
    String.raw`
  import {definePlugin} from '@lenso/core/plugin';
  import {createWorkerHandler,createBindingsPlugin} from '@lenso/workers';
  export default createWorkerHandler(bindings=>{
    const env=createBindingsPlugin({id:'bindings',bindings});
    const web=definePlugin({id:'web',requires:[env],setup(c){
      return {fetch:async()=>new Response(c.get(env).MESSAGE)};
    }});
    return {plugins:[env,web],web};
  });
`,
  );
  await Bun.write(
    join(workers, "lenso.engine.ts"),
    String.raw`
  import {moduleTarget} from 'lenso-example-module-target';
  export default {target:'workers',plugins:[moduleTarget({name:'workers',entry:'worker.ts'})]};
`,
  );
  const workersOutput = await build(workers);
  const handler = (await import(pathToFileURL(join(workersOutput, "worker.js")).href)).default;
  assert.equal(
    await (
      await handler.fetch(
        new Request("https://example.test"),
        { MESSAGE: "packaged workers" },
        { waitUntil() {} },
      )
    ).text(),
    "packaged workers",
  );
  const { definePlugin, startApp } = await import("@lenso/core");
  const { createBunListenerPlugin } = await import("@lenso/web/bun");
  const web = definePlugin({
    id: "packaged-web",
    setup: () => ({ fetch: async () => new Response("packaged Bun listener") }),
  });
  const listener = createBunListenerPlugin({
    web,
    hostname: "127.0.0.1",
    port: 0,
    ingress: (request, url) =>
      request.headers.get("origin") && request.headers.get("origin") !== url.origin
        ? new Response("Invalid origin", { status: 403 })
        : undefined,
  });
  const app = await startApp({ plugins: [web, listener] });
  const address = app.get(listener);
  try {
    assert.ok(address.port > 0);
    assert.equal(Number(address.url.port), address.port);
    assert.equal(await (await fetch(address.url)).text(), "packaged Bun listener");
    assert.equal(
      (await fetch(address.url, { headers: { origin: "https://untrusted.test" } })).status,
      403,
    );
  } finally {
    await app.stop();
  }
  const replacement = Bun.serve({
    hostname: "127.0.0.1",
    port: address.port,
    fetch: () => new Response("released"),
  });
  await replacement.stop(true);
  console.log("packaged consumer verified");
}
