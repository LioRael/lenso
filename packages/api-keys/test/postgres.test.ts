import { SQL } from "bun";
import { expect, test } from "bun:test";
import { access, mkdtemp, readFile, rm } from "node:fs/promises";
import { constants } from "node:fs";
import { createServer } from "node:net";
import { dirname, join } from "node:path";
import { drizzle } from "drizzle-orm/bun-sql";
import { postgresApiKeyStore } from "../src/drizzle/pg";
import { record, rotation, storeContract, subject } from "./drizzle.test";

const names = ["postgres", "initdb", "pg_ctl"] as const;
const binaries: Record<string, string> = {};
for (const name of names) {
  for (const path of [Bun.which(name), `/opt/homebrew/bin/${name}`]) {
    if (!path) continue;
    try {
      await access(path, constants.X_OK);
      binaries[name] = path;
      break;
    } catch {}
  }
}
const missing = names.filter((name) => !binaries[name]);
if (missing.length && process.env.LENSO_REQUIRE_POSTGRES === "1") {
  throw new Error(`Required PostgreSQL binaries unavailable: ${missing.join(", ")}`);
}
if (missing.length) console.warn(`Skipping real API key PostgreSQL: missing ${missing.join(", ")}`);
const pgTest = missing.length ? test.skip : test;

async function command(args: string[]) {
  const child = Bun.spawn(args, {
    env: { PATH: Bun.env.PATH ?? "/usr/bin:/bin", LC_ALL: "C" },
    stdout: "pipe",
    stderr: "pipe",
  });
  const [exit, stdout, stderr] = await Promise.all([
    child.exited,
    new Response(child.stdout).text(),
    new Response(child.stderr).text(),
  ]);
  if (exit !== 0) throw new Error(`Private PostgreSQL command failed: ${stdout}${stderr}`);
}

async function freePort() {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("No loopback port allocated");
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
  return address.port;
}

async function cluster(run: (owner: SQL, observer: SQL, a: SQL, b: SQL) => Promise<void>) {
  const root = await mkdtemp(
    join(process.env.DELTA_SCRATCH_DIR ?? dirname(import.meta.dir), ".api-keys-pg-"),
  );
  const data = join(root, "data");
  const clients: SQL[] = [];
  let startupAttempted = false;
  try {
    await command([
      binaries.initdb!,
      "-D",
      data,
      "-U",
      "api_keys_test",
      "--auth=trust",
      "--no-locale",
      "--encoding=UTF8",
    ]);
    const port = await freePort();
    startupAttempted = true;
    await command([
      binaries.pg_ctl!,
      "-D",
      data,
      "-l",
      join(root, "postgres.log"),
      "-w",
      "-t",
      "10",
      "-o",
      `-h 127.0.0.1 -p ${port} -k '' -c fsync=off`,
      "start",
    ]);
    for (let i = 0; i < 4; i++) {
      clients.push(
        new SQL({
          adapter: "postgres",
          hostname: "127.0.0.1",
          port,
          username: "api_keys_test",
          password: "",
          database: "postgres",
          max: 1,
          connectionTimeout: 5,
          idleTimeout: 0,
          tls: false,
        }),
      );
    }
    const [owner, observer, a, b] = clients as [SQL, SQL, SQL, SQL];
    for (const client of clients) await client`SET statement_timeout = '8s'`;
    await owner.unsafe(
      await readFile(new URL("../migrations/pg/0000_api_keys.sql", import.meta.url), "utf8"),
    );
    await run(owner, observer, a, b);
  } finally {
    try {
      for (const client of clients) await client.close({ timeout: 1 });
    } finally {
      try {
        if (
          startupAttempted &&
          (await access(join(data, "postmaster.pid")).then(
            () => true,
            () => false,
          ))
        ) {
          await command([
            binaries.pg_ctl!,
            "-D",
            data,
            "-w",
            "-t",
            "10",
            "-m",
            "immediate",
            "stop",
          ]);
        }
      } finally {
        await rm(root, { recursive: true, force: true });
      }
    }
  }
}

async function poll(check: () => Promise<boolean>) {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    if (await check()) return;
    await Bun.sleep(10);
  }
  throw new Error("Private PostgreSQL observation timed out");
}

pgTest(
  "real private PostgreSQL: API key store contract",
  async () => {
    await cluster(async (_owner, _observer, a, b) => {
      await storeContract(postgresApiKeyStore(drizzle(a)), postgresApiKeyStore(drizzle(b)));
    });
  },
  30_000,
);

pgTest(
  "real private PostgreSQL: distinct queued competing rotations have one winner",
  async () => {
    await cluster(async (owner, observer, a, b) => {
      const first = postgresApiKeyStore(drizzle(a));
      const second = postgresApiKeyStore(drizzle(b));
      const row = record("queued-competition");
      await first.create(row);
      const [pidA] = await a`SELECT pg_backend_pid() AS pid`;
      const [pidB] = await b`SELECT pg_backend_pid() AS pid`;
      expect(pidA.pid).not.toBe(pidB.pid);
      await owner`BEGIN`;
      await owner`SELECT id FROM api_keys WHERE id = ${row.id} FOR UPDATE`;
      const pending = [
        first.rotate(rotation(row, { digest: "competition-a" })),
        second.rotate(rotation(row, { digest: "competition-b" })),
      ];
      await poll(async () => {
        const rows = await observer`
        SELECT pid FROM pg_stat_activity WHERE pid IN ${observer([pidA.pid, pidB.pid])}
        AND wait_event_type = 'Lock' AND state = 'active'
      `;
        return rows.length === 2;
      });
      await owner`COMMIT`;
      expect((await Promise.all(pending)).filter(Boolean)).toHaveLength(1);
      expect((await first.read(row.id))?.revision).toBe(2);
      expect((await first.read(row.id))?.scopes).toEqual(row.scopes);
    });
  },
  30_000,
);

pgTest(
  "real private PostgreSQL: queued rotations evaluate expiry after owning row",
  async () => {
    await cluster(async (owner, observer, a, b) => {
      const first = postgresApiKeyStore(drizzle(a));
      const second = postgresApiKeyStore(drizzle(b));
      const row = record("locked-expiry", { expiresAt: Date.now() + 1_500 });
      await first.create(row);
      const [pidA] = await a`SELECT pg_backend_pid() AS pid`;
      const [pidB] = await b`SELECT pg_backend_pid() AS pid`;
      await owner`BEGIN`;
      await owner`SELECT id FROM api_keys WHERE id = ${row.id} FOR UPDATE`;
      const pending = [
        first.rotate(rotation(row, { digest: "queued-a" })),
        second.rotate(rotation(row, { digest: "queued-b" })),
      ];
      await poll(async () => {
        const rows = await observer`
        SELECT pid FROM pg_stat_activity WHERE pid IN ${observer([pidA.pid, pidB.pid])}
        AND wait_event_type = 'Lock' AND state = 'active'
      `;
        return rows.length === 2;
      });
      await poll(async () => {
        const [clock] =
          await observer`SELECT floor(extract(epoch from clock_timestamp()) * 1000)::bigint AS now`;
        return Number(clock.now) >= row.expiresAt;
      });
      await owner`COMMIT`;
      expect(await Promise.all(pending)).toEqual([null, null]);
      expect(await first.read(row.id)).toEqual(row);
    });
  },
  30_000,
);

for (const order of ["rotate-first", "revoke-first"] as const) {
  pgTest(
    `real private PostgreSQL: ${order} queued successor ends revoked`,
    async () => {
      await cluster(async (owner, observer, a, b) => {
        const first = postgresApiKeyStore(drizzle(a));
        const second = postgresApiKeyStore(drizzle(b));
        const row = record(order);
        await first.create(row);
        const [pidA] = await a`SELECT pg_backend_pid() AS pid`;
        const [pidB] = await b`SELECT pg_backend_pid() AS pid`;
        await owner`BEGIN`;
        await owner`SELECT id FROM api_keys WHERE id = ${row.id} FOR UPDATE`;
        const leading =
          order === "rotate-first"
            ? first.rotate(rotation(row, { overlapMs: 1_000 }))
            : first.revoke(subject, row.id, Date.now());
        const wait = async (pids: number[]) =>
          poll(async () => {
            const rows = await observer`
          SELECT pid FROM pg_stat_activity WHERE pid IN ${observer(pids)}
          AND wait_event_type = 'Lock' AND state = 'active'
        `;
            return rows.length === pids.length;
          });
        await wait([pidA.pid]);
        const trailing =
          order === "rotate-first"
            ? second.revoke(subject, row.id, Date.now())
            : second.rotate(rotation(row));
        await wait([pidA.pid, pidB.pid]);
        await owner`COMMIT`;
        const results = await Promise.all([leading, trailing]);
        if (order === "revoke-first") expect(results).toEqual([true, null]);
        else {
          expect(results[0]).not.toBeNull();
          expect(results[1]).toBe(true);
        }
        const revoked = await first.read(row.id);
        expect(revoked?.revokedAt).toBeNumber();
        expect(revoked?.previousDigest).toBeNull();
        expect(await second.revoke(subject, row.id, Date.now())).toBe(true);
        expect(await first.read(row.id)).toEqual(revoked);
      });
    },
    30_000,
  );
}
