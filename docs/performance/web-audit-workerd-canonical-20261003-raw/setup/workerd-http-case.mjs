// Prepared only. Run after exact artifact/runtime qualification in an allocated window.
import assert from 'node:assert/strict';
import http from 'node:http';
import { performance } from 'node:perf_hooks';

const base = new URL(process.env.LENSO_WORKERD_BENCH_URL ?? '');
assert(base.protocol === 'http:' && base.hostname === '127.0.0.1' && base.port &&
  base.pathname === '/' && !base.search && !base.hash && !base.username && !base.password);
const connections = Number(process.env.LENSO_WORKERD_BENCH_CONNECTIONS);
assert([1, 8].includes(connections));
const requests = 1000, warmup = 200;
const body = Buffer.from(Array.from({ length: 65536 }, (_, i) => i & 255));
const agent = new http.Agent({ keepAlive: true, maxSockets: connections, maxFreeSockets: connections });
async function request() {
  return new Promise((resolve, reject) => {
    const started = performance.now();
    let first;
    const req = http.request(new URL('/bytes', base), {
      method: 'POST', agent,
      headers: { 'content-length': body.length, 'content-type': 'application/octet-stream' },
    }, res => {
      const chunks = [];
      res.on('data', chunk => { first ??= performance.now(); chunks.push(chunk); });
      res.on('error', reject);
      res.on('end', () => {
        const complete = performance.now();
        try {
          assert.equal(res.statusCode, 200);
          assert(res.headers['x-request-id']);
          assert(Buffer.concat(chunks).equals(body));
          assert(first !== undefined);
          resolve({ first_byte_us: (first - started) * 1000, complete_us: (complete - started) * 1000 });
        } catch (error) { reject(error); }
      });
    });
    req.setTimeout(10000, () => req.destroy(new Error('bounded loopback request timeout')));
    req.on('error', reject);
    req.end(body);
  });
}
async function batch(count) {
  let next = 0;
  const raw = new Array(count);
  await Promise.all(Array.from({ length: connections }, async () => {
    while (next < count) { const index = next++; raw[index] = await request(); }
  }));
  return raw;
}
function summary(raw, key) {
  const sorted = raw.map(r => r[key]).sort((a, b) => a - b);
  return Object.fromEntries([50, 95, 99].map(p => ['p' + p, sorted[Math.floor(sorted.length * p / 100)]]));
}
try {
  const first_request = await request();
  await batch(warmup);
  const started = performance.now();
  const raw = await batch(requests);
  const elapsed_ms = performance.now() - started;
  process.stdout.write(JSON.stringify({
    schema: 'lenso.web-workerd-http-case.v1',
    scope: 'same-artifact real loopback workerd POST /bytes; five-route fixture; 64KiB',
    connections, body_bytes: body.length, requests, warmup, total_http_requests: 1 + warmup + requests,
    first_request, elapsed_ms, req_s: requests * 1000 / elapsed_ms,
    first_byte_us: summary(raw, 'first_byte_us'), complete_us: summary(raw, 'complete_us'), raw,
    uncertainty: 'warm closed-loop client including byte validation; no coordinated-omission correction; client/server CPU not separated',
  }) + '\n');
} finally { agent.destroy(); }
