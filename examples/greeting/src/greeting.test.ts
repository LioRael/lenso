import { expect, test } from 'bun:test';
import { startApp } from 'lenso';
import app from '../lenso.config';
import { greeting } from './greeting';
test('plain async service rejects invalid input without increasing state', async () => {
  const running = await startApp(app);
  try {
    const service = running.get(greeting);
    await expect(service.greet({ name: 'x' })).rejects.toThrow('at least 2');
    expect(await service.greet({ name: ' Ada ' })).toEqual({ message: 'Hello, Ada!', count: 1 });
  } finally { await running.stop(); }
});
