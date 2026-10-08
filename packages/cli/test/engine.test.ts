import { afterEach, describe, expect, test } from 'bun:test';
import { mkdtemp, mkdir, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { defineApp, definePlugin } from 'lenso';
import { discover, generate, invoke } from '../src/engine';

const directories: string[] = [];
afterEach(async () => {
  for (const directory of directories.splice(0)) await rm(directory, { recursive: true, force: true });
});
async function fixture(config: string): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), 'lenso-engine-'));
  directories.push(root);
  await Bun.write(join(root, 'lenso.config.ts'), config);
  return root;
}

describe('static assembly', () => {
  test('discovers topological order without initializing plugin business code', async () => {
    const root = await fixture(`
      const dependency = { id: 'dependency', setup() { throw new Error('setup must never run'); } };
      const dependent = { id: 'dependent', requires: [dependency], setup() { throw new Error('setup must never run'); } };
      export default { plugins: [dependent, dependency] };
    `);
    expect((await discover(root)).ordered.map(plugin => plugin.id)).toEqual(['dependency', 'dependent']);
  });

  test.each([
    ['duplicate identities', `export default { plugins: [{ id: 'a', setup() {} }, { id: 'a', setup() {} }] };`],
    ['missing dependencies', `const b = { id: 'b', setup() {} }; export default { plugins: [{ id: 'a', requires: [b], setup() {} }] };`],
    ['cycles', `const a = { id: 'a', requires: [], setup() {} }; const b = { id: 'b', requires: [a], setup() {} }; a.requires.push(b); export default { plugins: [a,b] };`],
  ])('rejects %s at discovery time', async (_name, config) => {
    const root = await fixture(config);
    await expect(discover(root)).rejects.toThrow();
    expect(await Bun.file(join(root, '.lenso/manifest.json')).exists()).toBe(false);
  });

  test('generates separate browser client and server entries', async () => {
    const root = await fixture(`export default { plugins: [{ id: 'greeting', setup() {}, contributions: [{ kind: 'example.metadata', label: 'Greeting' }] }] };`);
    await mkdir(join(root, 'src'));
    await Bun.write(join(root, 'src/router.ts'), 'export type AppRouter = {};');
    await generate(root);
    const client = await Bun.file(join(root, '.lenso/client.ts')).text();
    expect(client).toContain("from '@lenso/web/client'");
    expect(client).toContain('import type { AppRouter }');
    expect(client).not.toContain('lenso.config');
    expect(client).not.toContain('startApp');
    expect(await Bun.file(join(root, '.lenso/server.ts')).text()).toContain('../lenso.config');
  });


});

describe('direct service invocation', () => {
  test('calls async business methods and always closes acquired resources', async () => {
    const events: string[] = [];
    const greeting = definePlugin({
      id: 'greeting',
      setup(context) {
        events.push('start');
        context.onCleanup(() => { events.push('close'); });
        return { async greet(input: unknown) { return { message: `Hello ${String(input)}!` }; } };
      },
    });
    expect(await invoke(defineApp({ plugins: [greeting] }), 'greeting', 'greet', 'Ada')).toEqual({ message: 'Hello Ada!' });
    expect(events).toEqual(['start', 'close']);
  });

  test('closes resources on business failure and rejects prototype methods', async () => {
    let closed = 0;
    const plugin = definePlugin({ id: 'failure', setup(context) {
      context.onCleanup(() => { closed++; });
      return { async run() { throw new Error('business failure'); } };
    } });
    const app = defineApp({ plugins: [plugin] });
    await expect(invoke(app, 'failure', 'run', {})).rejects.toThrow('business failure');
    await expect(invoke(app, 'failure', 'toString', {})).rejects.toThrow('does not expose method');
    expect(closed).toBe(2);
  });
});
