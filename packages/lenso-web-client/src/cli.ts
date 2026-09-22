#!/usr/bin/env node
import { generateClientTypes } from './generate.js';

const [command, input, output, ...rest] = process.argv.slice(2);
if (command !== 'generate' || input === undefined || output === undefined || rest.length > 0) {
  console.error('Usage: lenso-web-client generate <openapi.json> <generated.ts>');
  process.exitCode = 2;
} else {
  try {
    const result = await generateClientTypes({ input, output });
    console.log(`Generated ${result.operationCount} public operation(s); source sha256=${result.digest}`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
