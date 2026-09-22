import { afterEach, describe, expect, test } from 'bun:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { generateClientTypes } from '../dist/generate.js';

const temporaryDirectories: string[] = [];

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((directory) => rm(directory, { force: true, recursive: true })));
});

describe('client generation', () => {
  test('emits only operations in the explicit public OpenAPI document', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'lenso-web-client-'));
    temporaryDirectories.push(directory);
    const input = join(directory, 'openapi.json');
    const output = join(directory, 'generated', 'api.ts');
    await writeFile(input, JSON.stringify(publicDocument()));

    const result = await generateClientTypes({ input, output });
    const generated = await readFile(output, 'utf8');

    expect(result.operationCount).toBe(2);
    expect(generated).toContain('"/notes"');
    expect(generated).toContain('notes.list');
    expect(generated).not.toContain('lenso.config');
    expect(generated).toContain(`Source SHA-256: ${result.digest}`);
  });

  test('rejects unstable and unsupported public documents', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'lenso-web-client-'));
    temporaryDirectories.push(directory);
    const input = join(directory, 'openapi.json');
    const output = join(directory, 'api.ts');
    const document: any = publicDocument();
    delete document.paths['/notes'].get.operationId;
    await writeFile(input, JSON.stringify(document));
    await expect(generateClientTypes({ input, output })).rejects.toThrow('stable operationId');

    document.paths['/notes'].get.operationId = 'notes.list';
    document.webhooks = { changed: {} };
    await writeFile(input, JSON.stringify(document));
    await expect(generateClientTypes({ input, output })).rejects.toThrow('webhooks are not supported');
  });

  test('rejects external references before the generator can read them', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'lenso-web-client-'));
    temporaryDirectories.push(directory);
    const input = join(directory, 'openapi.json');
    const output = join(directory, 'api.ts');
    const document: any = publicDocument();
    document.components = { schemas: { Note: { $ref: 'https://metadata.invalid/schema.json' } } };
    await writeFile(input, JSON.stringify(document));
    await expect(generateClientTypes({ input, output })).rejects.toThrow('self-contained');
  });
});

function publicDocument() {
  return {
    openapi: '3.1.0',
    info: { title: 'Knowledge Base', version: '1.0.0' },
    paths: {
      '/notes': {
        get: {
          operationId: 'notes.list',
          responses: { '200': { description: 'Notes', content: { 'application/json': { schema: { type: 'object' } } } } },
        },
        post: {
          operationId: 'notes.create',
          responses: { '201': { description: 'Created', content: { 'application/json': { schema: { type: 'object' } } } } },
        },
      },
    },
  };
}
