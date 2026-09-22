import { resolve } from 'node:path';
import type { BunPlugin } from 'bun';

const typescript5 = resolve(import.meta.dir, '../../../node_modules/typescript-5/lib/typescript.js');
const typescriptCompatibility: BunPlugin = {
  name: 'openapi-typescript-typescript-5',
  setup(build) {
    build.onResolve({ filter: /^typescript$/ }, () => ({ path: typescript5 }));
  },
};

await build({
  entrypoints: [resolve(import.meta.dir, '../src/index.ts')],
  external: ['openapi-fetch'],
  target: 'browser',
});
await build({
  entrypoints: [resolve(import.meta.dir, '../src/generate.ts'), resolve(import.meta.dir, '../src/cli.ts')],
  plugins: [typescriptCompatibility],
  splitting: true,
  target: 'node',
});

async function build(options: Parameters<typeof Bun.build>[0]): Promise<void> {
  const result = await Bun.build({
    ...options,
    format: 'esm',
    naming: '[name].js',
    outdir: resolve(import.meta.dir, '../dist'),
  });
  if (!result.success) throw new AggregateError(result.logs, 'Could not build @lenso/web-client');
}
