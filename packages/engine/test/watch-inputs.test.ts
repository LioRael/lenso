import { afterEach, expect, test } from "bun:test";
import { mkdtemp, mkdir, realpath, rm, symlink } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { SourceInputs } from "../src/source-inputs";
import { InputWatches } from "../src/input-watches";

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "lenso-watch-inputs-"));
  roots.push(root);
  return realpath(root);
}
async function until(check: () => boolean) {
  const deadline = Date.now() + 4000;
  while (!check()) {
    if (Date.now() > deadline) throw new Error("Real filesystem watch did not settle");
    await Bun.sleep(20);
  }
}

test("source inputs follow linked/local graphs, imported JSON and config extends but never scan third-party subgraphs", async () => {
  const root = await fixture();
  const linked = await fixture();
  await Bun.write(join(linked, "package.json"), '{"name":"linked","exports":"./code/entry.ts"}');
  await Bun.write(join(linked, "code/entry.ts"), "export {value} from './nested'");
  await Bun.write(
    join(linked, "code/nested.ts"),
    "import data from './value.json'; export const value = data.value",
  );
  await Bun.write(join(linked, "code/value.json"), '{"value":"linked"}');
  await Bun.write(
    join(root, "node_modules/third/package.json"),
    '{"name":"third","exports":"./index.ts"}',
  );
  await Bun.write(join(root, "node_modules/third/index.ts"), "export {value} from './hidden'");
  await Bun.write(join(root, "node_modules/third/hidden.ts"), "export const value = 'third'");
  await symlink(linked, join(root, "node_modules/linked"));
  await Bun.write(join(root, "tsconfig.json"), '{/* JSONC */ "extends":"./config/base"}');
  await Bun.write(join(root, "config/base.json"), '{"compilerOptions":{}}');
  await Bun.write(
    join(root, "entry.ts"),
    "import 'linked'; import 'third'; import './outside/leaf';",
  );
  await Bun.write(join(root, "outside/leaf.ts"), "export const leaf = true");
  await Bun.write(join(root, "business-data.json"), '{"not":"imported"}');
  const inputs = new SourceInputs(root);
  await inputs.configuration(root);
  await inputs.add(join(root, "entry.ts"));
  expect(inputs.files).toContain(join(linked, "code/nested.ts"));
  expect(inputs.files).toContain(join(linked, "code/value.json"));
  expect(inputs.files).toContain(join(root, "outside/leaf.ts"));
  expect(inputs.files).toContain(join(root, "config/base.json"));
  expect(inputs.files).toContain(join(root, "node_modules/linked/package.json"));
  expect(inputs.files).not.toContain(join(root, "business-data.json"));
  expect(inputs.files).not.toContain(join(root, "node_modules/third/hidden.ts"));
});

test("declared local links are followed even inside the same dependency tree", async () => {
  const root = await fixture();
  const local = join(root, "node_modules/local-project");
  await Bun.write(
    join(root, "package.json"),
    '{"dependencies":{"linked":"link:./node_modules/local-project"}}',
  );
  await Bun.write(join(local, "package.json"), '{"name":"linked","main":"entry.ts"}');
  await Bun.write(join(local, "entry.ts"), "export {default} from './value.json'");
  await Bun.write(join(local, "value.json"), '"local"');
  await symlink(local, join(root, "node_modules/linked"));
  await Bun.write(join(root, "entry.ts"), "import 'linked'");
  const inputs = new SourceInputs(root);
  await inputs.add(join(root, "entry.ts"));
  expect(inputs.files).toContain(join(local, "value.json"));
  expect(inputs.directories).toContain(local);
});

test("graph and explicit watches share listeners, replace ownership and close real fs.watch resources", async () => {
  const root = await fixture();
  const code = join(root, "code/entry.ts");
  const data = join(root, "code/business.json");
  await Bun.write(code, "export const value = 1");
  await Bun.write(data, "{}");
  let changes = 0;
  const errors: unknown[] = [];
  const watches = new InputWatches(
    root,
    () => changes++,
    (error) => errors.push(error),
  );
  try {
    watches.replace([join(root, "code"), code, code]);
    watches.replace([join(root, "code"), code]);
    await Bun.write(data, '{"explicit":true}');
    await until(() => changes > 0);
    await Bun.sleep(100);
    const prior = changes;
    watches.replace([code]);
    await Bun.write(data, '{"ordinary":true}');
    await Bun.sleep(250);
    expect(changes).toBe(prior);
    await Bun.write(code, "export const value = 2");
    await until(() => changes > prior);
    await Bun.sleep(100);
    const beforeClose = changes;
    watches.close();
    await Bun.write(code, "export const value = 3");
    await Bun.sleep(250);
    expect(changes).toBe(beforeClose);
    expect(errors).toEqual([]);
  } finally {
    watches.close();
  }
});

test("nearest existing parent watches repair new nested source and missing package manifests", async () => {
  const root = await fixture();
  await Bun.write(
    join(root, "entry.ts"),
    "import './new/nested/module'; import 'missing-package';",
  );
  const inputs = new SourceInputs(root);
  await inputs.add(join(root, "entry.ts"));
  let changes = 0;
  const errors: unknown[] = [];
  const watches = new InputWatches(
    root,
    () => changes++,
    (error) => errors.push(error),
  );
  try {
    watches.replace([...inputs.files]);
    await Bun.write(join(root, "new/nested/module.ts"), "export const value = 1");
    await until(() => changes > 0);
    await Bun.sleep(100);
    const prior = changes;
    await Bun.write(
      join(root, "node_modules/missing-package/package.json"),
      '{"name":"missing-package","main":"entry.js"}',
    );
    await until(() => changes > prior);
    expect(errors).toEqual([]);
  } finally {
    watches.close();
  }
});

test("new and deleted source in graph directories invalidate but existing unrelated source edits do not", async () => {
  const root = await fixture();
  await Bun.write(join(root, "entry.ts"), "export {}");
  const unrelated = join(root, "unrelated.ts");
  await Bun.write(unrelated, "export const value = 1");
  let changes = 0;
  const watches = new InputWatches(
    root,
    () => changes++,
    (error) => {
      throw error;
    },
  );
  try {
    watches.replace([join(root, "entry.ts")], [root]);
    await Bun.write(unrelated, "export const value = 2");
    await Bun.sleep(250);
    expect(changes).toBe(0);
    const added = join(root, "added.ts");
    await Bun.write(added, "export {}");
    await until(() => changes > 0);
    await Bun.sleep(100);
    const prior = changes;
    await rm(added);
    await until(() => changes > prior);
  } finally {
    watches.close();
  }
});

test("missing-leaf probes replan when ancestors appear before the leaf is created", async () => {
  const root = await fixture();
  const leaf = join(root, "new/nested/leaf.ts");
  let changes = 0;
  const watches = new InputWatches(
    root,
    () => changes++,
    (error) => {
      throw error;
    },
  );
  try {
    watches.replace([leaf]);
    await mkdir(join(root, "new"));
    await until(() => changes > 0);
    watches.replace([leaf]);
    const first = changes;
    await mkdir(join(root, "new/nested"));
    await until(() => changes > first);
    watches.replace([leaf]);
    const second = changes;
    await Bun.write(leaf, "export {}");
    await until(() => changes > second);
  } finally {
    watches.close();
  }
});

test("a package manifest with an absent entry retains that entry as a repair input without scanning source", async () => {
  const root = await fixture();
  await Bun.write(join(root, "entry.ts"), "import 'incomplete'");
  await Bun.write(
    join(root, "node_modules/incomplete/package.json"),
    '{"name":"incomplete","main":"missing.js"}',
  );
  const entry = join(root, "node_modules/incomplete/missing.js");
  const inputs = new SourceInputs(root);
  await inputs.add(join(root, "entry.ts"));
  expect(inputs.files).toContain(entry);
  let changes = 0;
  const watches = new InputWatches(
    root,
    () => changes++,
    (error) => {
      throw error;
    },
  );
  try {
    watches.replace([...inputs.files]);
    await Bun.write(entry, "export {}");
    await until(() => changes > 0);
  } finally {
    watches.close();
  }
});
