import { afterEach, expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

test("bundle helper inherits source resolution instead of silently selecting stale dist", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-bundle-conditions-"));
  roots.push(root);
  await Bun.write(join(root, "lenso.config.ts"), "export default {plugins:[]}");
  await Bun.write(
    join(root, "node_modules/marker/package.json"),
    JSON.stringify({
      name: "marker",
      type: "module",
      exports: { "lenso-source": "./src.ts", default: "./dist.js" },
    }),
  );
  await Bun.write(join(root, "node_modules/marker/src.ts"), "export default 'CURRENT_SOURCE'");
  await Bun.write(join(root, "node_modules/marker/dist.js"), "export default 'STALE_DIST'");
  await Bun.write(join(root, "entry.ts"), "import marker from 'marker'; console.log(marker)");
  await Bun.write(
    join(root, "lenso.engine.ts"),
    `export default {target:'custom',plugins:[{name:'custom',setup(c){
      c.target('custom',c=>c.bundle({entry:c.entry,packages:'bundle'}));
    }}]}`,
  );
  const build = resolve(import.meta.dir, "../src/engine.ts");
  const driver = join(root, "driver.ts");
  await Bun.write(
    driver,
    `import marker from 'marker'; import {build} from ${JSON.stringify(build)};
    console.log('ordinary:'+marker); await build(${JSON.stringify(root)},'entry.ts');`,
  );
  const child = Bun.spawn([process.execPath, "--conditions=lenso-source", driver], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const stderr = await new Response(child.stderr).text();
  expect(await child.exited).toBe(0);
  expect(stderr).toBe("");
  expect(await new Response(child.stdout).text()).toBe("ordinary:CURRENT_SOURCE\n");
  const artifact = Bun.spawn([process.execPath, join(root, "dist/entry.js")], {
    stdout: "pipe",
    stderr: "pipe",
  });
  expect(await artifact.exited).toBe(0);
  expect(await new Response(artifact.stdout).text()).toBe("CURRENT_SOURCE\n");
});

test("custom targets replace inherited conditions and retain platform defaults in runnable bundles", async () => {
  for (const scenario of [
    { flags: [], options: {}, expected: "BUN", conditions: [] },
    {
      flags: ["--conditions=lenso-source,custom"],
      options: {},
      expected: "BUN",
      conditions: ["lenso-source,custom"],
    },
    {
      flags: ["--conditions=lenso-source"],
      options: { conditions: [] },
      expected: "BUN",
      conditions: ["lenso-source"],
    },
    {
      flags: ["--conditions=lenso-source"],
      options: { conditions: ["custom"] },
      expected: "CUSTOM",
      conditions: ["lenso-source"],
    },
    { flags: ["--conditions=custom"], options: {}, expected: "CUSTOM", conditions: ["custom"] },
    {
      flags: ["--conditions=lenso-source"],
      options: { platform: "node", conditions: [] },
      expected: "NODE",
      conditions: ["lenso-source"],
    },
    {
      flags: ["--conditions=lenso-source"],
      options: { platform: "browser", conditions: [] },
      expected: "BROWSER",
      conditions: ["lenso-source"],
    },
  ]) {
    const root = await mkdtemp(join(tmpdir(), "lenso-bundle-target-"));
    roots.push(root);
    await Bun.write(join(root, "lenso.config.ts"), "export default {plugins:[]}");
    await Bun.write(
      join(root, "node_modules/marker/package.json"),
      JSON.stringify({
        name: "marker",
        type: "module",
        exports: {
          "lenso-source": "./source.ts",
          custom: "./custom.ts",
          bun: "./bun.js",
          node: "./node.js",
          browser: "./browser.js",
          default: "./default.js",
        },
      }),
    );
    for (const [path, value] of Object.entries({
      "source.ts": "SOURCE",
      "custom.ts": "CUSTOM",
      "bun.js": "BUN",
      "node.js": "NODE",
      "browser.js": "BROWSER",
      "default.js": "PUBLICATION",
    }))
      await Bun.write(
        join(root, "node_modules/marker", path),
        `export default ${JSON.stringify(value)}`,
      );
    await Bun.write(join(root, "entry.ts"), "import value from 'marker'; console.log(value)");
    await Bun.write(
      join(root, "lenso.engine.ts"),
      `
      export default {target:'custom',plugins:[{name:'custom',setup(c){
        c.target('custom',async c=>{
          console.log(JSON.stringify(c.conditions));
          return c.bundle({entry:c.entry,packages:'bundle',...${JSON.stringify(scenario.options)}});
        });
      }}]}`,
    );
    const driver = join(root, "driver.ts");
    await Bun.write(
      driver,
      `import {build} from ${JSON.stringify(resolve(import.meta.dir, "../src/engine.ts"))};
      await build(${JSON.stringify(root)},'entry.ts');`,
    );
    const child = Bun.spawn([process.execPath, ...scenario.flags, driver], {
      stdout: "pipe",
      stderr: "pipe",
    });
    const stderr = await new Response(child.stderr).text();
    expect(await child.exited).toBe(0);
    expect(stderr).toBe("");
    expect(JSON.parse(await new Response(child.stdout).text())).toEqual(scenario.conditions);
    const artifact = Bun.spawn([process.execPath, join(root, "dist/entry.js")], {
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(await artifact.exited).toBe(0);
    expect(await new Response(artifact.stdout).text()).toBe(scenario.expected + "\n");
  }
});

test("external artifacts retain runtime imports and let the runtime select source or publication", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-bundle-external-"));
  roots.push(root);
  await Bun.write(join(root, "lenso.config.ts"), "export default {plugins:[]}");
  await Bun.write(
    join(root, "node_modules/marker/package.json"),
    JSON.stringify({
      name: "marker",
      type: "module",
      exports: { "lenso-source": "./src.ts", default: "./dist.js" },
    }),
  );
  await Bun.write(join(root, "node_modules/marker/src.ts"), "export default 'CURRENT_SOURCE'");
  await Bun.write(join(root, "node_modules/marker/dist.js"), "export default 'PUBLICATION'");
  await Bun.write(join(root, "entry.ts"), "import marker from 'marker'; console.log(marker)");
  await Bun.write(
    join(root, "lenso.engine.ts"),
    `
    export default {target:'external',plugins:[{name:'external',setup(c){
      c.target('external',c=>c.bundle({entry:c.entry}));
    }}]}`,
  );
  const driver = join(root, "driver.ts");
  await Bun.write(
    driver,
    `import {build} from ${JSON.stringify(resolve(import.meta.dir, "../src/engine.ts"))};
    await build(${JSON.stringify(root)},'entry.ts');`,
  );
  const child = Bun.spawn([process.execPath, "--conditions=lenso-source", driver], {
    stdout: "pipe",
    stderr: "pipe",
  });
  expect(await child.exited).toBe(0);
  expect(await new Response(child.stderr).text()).toBe("");
  expect(await Bun.file(join(root, "dist/entry.js")).text()).toContain('from "marker"');
  for (const [flags, expected] of [
    [[], "PUBLICATION"],
    [["--conditions=lenso-source"], "CURRENT_SOURCE"],
  ] as const) {
    const artifact = Bun.spawn([process.execPath, ...flags, join(root, "dist/entry.js")], {
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(await artifact.exited).toBe(0);
    expect(await new Response(artifact.stdout).text()).toBe(expected + "\n");
  }
});
