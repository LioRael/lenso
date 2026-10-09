import { afterAll, beforeAll, expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { createServer, request } from 'node:https';
import { checkServerIdentity } from 'node:tls';
import { createPinnedHttpsTransport, validateEndpointUrl } from '../src/network';

const policy = {
  allowedHosts: ['webhook.invalid'], dnsTimeoutMs: 100, connectTimeoutMs: 150,
  timeoutMs: 300, maxRequestBytes: 32, maxResponseBytes: 32,
};
let directory: string;
beforeAll(() => {
  directory = mkdtempSync(new URL('../node_modules/lenso-webhook-tls-', import.meta.url).pathname);
  const result = Bun.spawnSync(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes',
    '-keyout', join(directory, 'key.pem'), '-out', join(directory, 'cert.pem'),
    '-days', '1', '-subj', '/CN=webhook.invalid', '-addext', 'subjectAltName=DNS:webhook.invalid']);
  expect(result.exitCode).toBe(0);
});
afterAll(() => rmSync(directory, { recursive: true, force: true }));

test('endpoint canonicalization has an exact nonempty allowlist and rejects unsafe authorities', () => {
  expect(validateEndpointUrl('https://WEBHOOK.invalid:443/a?x=%20', policy))
    .toBe('https://webhook.invalid/a?x=%20');
  for (const url of [
    'http://webhook.invalid', 'https://webhook.invalid:444', 'https://user:pass@webhook.invalid',
    'https://webhook.invalid#', 'https://webhook.invalid/#secret', 'https://webhook.invalid.',
    'https://sub.webhook.invalid', 'https://other.invalid', 'https://127.0.0.1',
    'https://2130706433', 'https://0x7f000001', 'https://0177.0.0.1',
    'https://[::1]', 'https://[::ffff:127.0.0.1]', 'https://%77ebhook.invalid',
    'https://webhook.invalid\\@other.invalid', ' https://webhook.invalid',
  ]) expect(() => validateEndpointUrl(url, policy)).toThrow('Webhook outbound policy-rejected');
  expect(() => validateEndpointUrl('https://webhook.invalid', { ...policy, allowedHosts: [] })).toThrow();
  expect(() => createPinnedHttpsTransport({ ...policy, timeoutMs: 0 })).toThrow();
});

test('Bun HTTPS lookup and numeric TLS hostname verification are assessed', async () => {
  const server = createServer({
    key: readFileSync(join(directory, 'key.pem')), cert: readFileSync(join(directory, 'cert.pem')),
  }, (_req, res) => res.end());
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  let lookupCalls = 0;
  try {
    const address = server.address() as { port: number };
    const outcome = await new Promise<string>(resolve => {
      const req = request({
        hostname: 'webhook.invalid', port: address.port, rejectUnauthorized: false, agent: false,
        lookup: (_host, _options, callback) => {
          lookupCalls++;
          if (_options.all) {
            (callback as unknown as (error: null, addresses: { address: string; family: number }[]) => void)(
              null, [{ address: '127.0.0.1', family: 4 }],
            );
          } else callback(null, '127.0.0.1', 4);
        },
      }, response => { response.resume(); response.on('end', () => resolve('connected')); });
      req.on('error', () => resolve('failed'));
      req.setTimeout(1000, () => { req.destroy(); resolve('timeout'); });
      req.end();
    });
    console.info(`Bun ${Bun.version} HTTPS lookup probe: calls=${lookupCalls}, outcome=${outcome}`);
    const numericProbe = async (host: string) => {
      let checks = 0;
      const result = await new Promise<string>(resolve => {
        const req = request({
          hostname: '127.0.0.1', port: address.port, servername: host, agent: false,
          ca: readFileSync(join(directory, 'cert.pem')), rejectUnauthorized: true,
          headers: { host },
          checkServerIdentity: (_hostname, cert) => {
            checks++;
            return checkServerIdentity(host, cert);
          },
        }, response => { response.resume(); response.on('end', () => resolve('connected')); });
        req.on('error', () => resolve('failed'));
        req.setTimeout(1000, () => { req.destroy(); resolve('timeout'); });
        req.end();
      });
      return { result, checks };
    };
    const correct = await numericProbe('webhook.invalid');
    const wrong = await numericProbe('wrong.invalid');
    console.info(`Bun numeric TLS probe: correct=${JSON.stringify(correct)}, wrong=${JSON.stringify(wrong)}`);
    expect(outcome).toBe('connected');
    expect(lookupCalls).toBe(1);
    expect(correct).toEqual({ result: 'connected', checks: 1 });
    expect(wrong).toEqual({ result: 'failed', checks: 1 });
  } finally { await new Promise<void>(resolve => server.close(() => resolve())); }
});

test.each(['node', 'bun'])('%s actual pinned TLS workflow, deadlines, size limits and sanitized failures', async runtime => {
  const moduleUrl = new URL('../src/network.ts', import.meta.url).href;
  const script = `
    import assert from 'node:assert/strict';
    import { createServer } from 'node:https';
    import { createServer as createTcpServer } from 'node:net';
    import { readFileSync } from 'node:fs';
    import { createPinnedHttpsTransportForTest } from ${JSON.stringify(moduleUrl)};
    const policy = ${JSON.stringify(policy)};
    const ca = readFileSync(${JSON.stringify(join(directory, 'cert.pem'))}, 'utf8');
    const key = readFileSync(${JSON.stringify(join(directory, 'key.pem'))});
    let received = 0;
    const server = createServer({ key, cert: ca }, (req, res) => {
      received++;
      assert.equal(req.headers.host, 'webhook.invalid');
      assert.equal(req.socket.servername, 'webhook.invalid');
      req.resume();
      if (req.url === '/redirect') { res.writeHead(302, { location: 'https://private.invalid/secret' }); res.end('private response'); }
      else if (req.url === '/large') res.end('private response body'.repeat(20));
      else if (req.url === '/slow') { res.writeHead(200); res.flushHeaders(); }
      else { res.writeHead(204, { 'retry-after': '12' }); res.end(); }
    });
    await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
    const loopbackTlsPort = server.address().port;
    const sendInput = path => ({ url: 'https://webhook.invalid' + path, body: new Uint8Array([1, 2]), headers: {}, signal: new AbortController().signal });
    const expectError = async (promise, code) => {
      await assert.rejects(promise, error => {
        assert.equal(error.code, code);
        assert.equal(error.message, 'Webhook outbound ' + code);
        assert.equal(error.cause, undefined);
        assert.ok(!JSON.stringify(error).includes('private'));
        return true;
      });
    };
    let calls = 0;
    const transport = createPinnedHttpsTransportForTest(policy, {
      ca, loopbackTlsPort, isPublicAddress: () => true,
      resolve: async () => [{ address: ++calls === 1 ? '127.0.0.1' : '127.0.0.2', family: 4 }],
    });
    try {
      assert.deepEqual(await transport.send(sendInput('/')), { status: 204, retryAfter: '12' });
      assert.equal(calls, 1);
      assert.equal(received, 1);
      await assert.rejects(transport.send(sendInput('/')), error => ['timeout', 'connection-failed'].includes(error.code));
      assert.equal(calls, 2);
      assert.equal(received, 1);
      const local = createPinnedHttpsTransportForTest(policy, {
        ca, loopbackTlsPort, isPublicAddress: () => true, resolve: async () => [{ address: '127.0.0.1', family: 4 }],
      });
      await expectError(local.send(sendInput('/redirect')), 'redirect-rejected');
      await expectError(local.send(sendInput('/large')), 'response-too-large');
      await expectError(local.send(sendInput('/slow')), 'timeout');
      await expectError(local.send({ ...sendInput('/'), body: new Uint8Array(33) }), 'request-too-large');
      await expectError(local.send({ ...sendInput('/'), headers: { Host: 'private.invalid' } }), 'policy-rejected');
      const mixed = createPinnedHttpsTransportForTest(policy, {
        ca, loopbackTlsPort, isPublicAddress: address => address !== '127.0.0.2',
        resolve: async () => [{ address: '127.0.0.1', family: 4 }, { address: '127.0.0.2', family: 4 }],
      });
      await expectError(mixed.send(sendInput('/')), 'policy-rejected');
      const hanging = createPinnedHttpsTransportForTest(policy, {
        ca, loopbackTlsPort, isPublicAddress: () => true, resolve: () => new Promise(() => {}),
      });
      await expectError(hanging.send(sendInput('/')), 'timeout');
      const sockets = new Set();
      const stalled = createTcpServer(socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
      await new Promise(resolve => stalled.listen(0, '127.0.0.1', resolve));
      try {
        const noHandshake = createPinnedHttpsTransportForTest({ ...policy, connectTimeoutMs: 40 }, {
          ca, loopbackTlsPort: stalled.address().port, isPublicAddress: () => true,
          resolve: async () => [{ address: '127.0.0.1', family: 4 }],
        });
        await expectError(noHandshake.send(sendInput('/')), 'timeout');
      } finally {
        for (const socket of sockets) socket.destroy();
        await new Promise(resolve => stalled.close(resolve));
      }
      const total = createPinnedHttpsTransportForTest({ ...policy, dnsTimeoutMs: 200, timeoutMs: 60 }, {
        ca, loopbackTlsPort, isPublicAddress: () => true,
        resolve: () => new Promise(resolve => setTimeout(() => resolve([{ address: '127.0.0.1', family: 4 }]), 100)),
      });
      const beforeTotal = received;
      await expectError(total.send(sendInput('/')), 'timeout');
      await new Promise(resolve => setTimeout(resolve, 70));
      assert.equal(received, beforeTotal);
      const aborted = new AbortController(); aborted.abort('private raw error');
      await expectError(local.send({ ...sendInput('/'), signal: aborted.signal }), 'timeout');
      const wrongHost = createPinnedHttpsTransportForTest({ ...policy, allowedHosts: ['wrong.invalid'] }, {
        ca, loopbackTlsPort, isPublicAddress: () => true, resolve: async () => [{ address: '127.0.0.1', family: 4 }],
      });
      const beforeBadTls = received;
      await expectError(wrongHost.send({ ...sendInput('/'), url: 'https://wrong.invalid/' }), 'connection-failed');
      const untrusted = createPinnedHttpsTransportForTest(policy, {
        ca: '', loopbackTlsPort, isPublicAddress: () => true, resolve: async () => [{ address: '127.0.0.1', family: 4 }],
      });
      await expectError(untrusted.send(sendInput('/')), 'connection-failed');
      assert.equal(received, beforeBadTls);
      for (const address of ['0.0.0.0', '10.0.0.1', '127.0.0.1', '169.254.169.254',
        '168.63.129.16', '172.16.0.1', '192.168.1.1', '100.64.0.1', '198.18.0.1', '192.0.2.1',
        '198.51.100.1', '203.0.113.1', '224.0.0.1', '255.255.255.255', '::1', 'fc00::1',
        'fe80::1', '::ffff:7f00:1', '::ffff:127.0.0.1', '2001::1', '2001:100::1',
        '2001:db8::1', '2002:0808:0808::1', '3fff::1']) {
        const denied = createPinnedHttpsTransportForTest(policy, {
          ca, resolve: async () => [{ address, family: address.includes(':') ? 6 : 4 }],
        });
        await expectError(denied.send(sendInput('/')), 'policy-rejected');
      }
      console.log(${JSON.stringify(runtime)} + ' numeric-address pinning, original Host/SNI, CA and hostname verification passed');
    } finally { server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
  `;
  const command = runtime === 'node' ? ['node', '--input-type=module', '-e', script] : ['bun', '-e', script];
  const child = Bun.spawn(command, { stdout: 'pipe', stderr: 'pipe' });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(child.stdout).text(), new Response(child.stderr).text(), child.exited,
  ]);
  console.info(stdout);
  if (exitCode !== 0) console.error(stderr);
  expect(exitCode).toBe(0);
}, 10000);
