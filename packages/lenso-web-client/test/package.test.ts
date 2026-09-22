import { expect, test } from 'bun:test';
import { access, readFile } from 'node:fs/promises';

test('package exposes browser runtime and generation entrypoints', async () => {
  await access(new URL('../dist/index.js', import.meta.url));
  await access(new URL('../dist/index.d.ts', import.meta.url));
  await access(new URL('../dist/generate.js', import.meta.url));
  const cli = await readFile(new URL('../dist/cli.js', import.meta.url), 'utf8');
  expect(cli.startsWith('#!/usr/bin/env node')).toBeTrue();
});
