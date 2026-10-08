import { SQL } from "bun";
import { expect, test } from "bun:test";
import { access, mkdtemp, readFile, rm } from "node:fs/promises";
import { constants } from "node:fs";
import { createServer } from "node:net";
import { dirname, join } from "node:path";
import { drizzle } from "drizzle-orm/bun-sql";
import { postgresSessionStore } from "../src/drizzle/pg";
import { createManagedSessions, type ManagedSession } from "../src/sessions";

const required = process.env.LENSO_REQUIRE_POSTGRES === "1";
const binaryNames = ["postgres", "initdb", "pg_ctl"] as const;
const binaries: Record<string, string> = {};
for (const name of binaryNames) {
  const candidates = [Bun.which(name), `/opt/homebrew/bin/${name}`];
  for (const path of candidates) {
    if (!path) continue;
    try {
      await access(path, constants.X_OK);
      binaries[name] = path;
      break;
    } catch {}
  }
}
const missing = binaryNames.filter((name) => !binaries[name]);
if (missing.length && required) {
  throw new Error(`Required PostgreSQL integration binaries unavailable: ${missing.join(", ")}`);
}
if (missing.length) {
  console.warn(
    `Skipping real PostgreSQL tests: installed binaries unavailable (${missing.join(", ")}). Set LENSO_REQUIRE_POSTGRES=1 to require them.`,
  );
}
const pgTest = missing.length ? test.skip : test;
const context = { signal: new AbortController().signal };

async function command(args: string[]) {
  const process = Bun.spawn(args, {
    env: { PATH: Bun.env.PATH ?? "/usr/bin:/bin", LC_ALL: "C" },
    stdout: "pipe",
    stderr: "pipe",
  });
  const [exit, stdout, stderr] = await Promise.all([
    process.exited,
    new Response(process.stdout).text(),
    new Response(process.stderr).text(),
  ]);
  if (exit !== 0) throw new Error(`${args[0]} failed (${exit}): ${stdout}${stderr}`);
  return stdout.trim();
}

async function freePort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("No loopback port allocated");
  const port = address.port;
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
  return port;
}

async function poll(label: string, check: () => Promise<boolean>, timeout = 5_000) {
  const deadline = Date.now() + timeout;
  do {
    if (await check()) return;
    await Bun.sleep(10);
  } while (Date.now() < deadline);
  throw new Error(`Timed out waiting for ${label}`);
}

type Manager = ReturnType<typeof createManagedSessions<undefined, string>>;

async function cluster(
  run: (fixture: {
    owner: SQL;
    observer: SQL;
    first: Manager;
    second: Manager;
    store: ReturnType<typeof postgresSessionStore>;
    waitFor: (clients: ("first" | "second")[]) => Promise<void>;
    readyToRenew: (issued: ManagedSession) => Promise<void>;
    short: Manager;
  }) => Promise<void>,
) {
  const root = await mkdtemp(
    join(process.env.DELTA_SCRATCH_DIR ?? dirname(import.meta.dir), ".auth-pg-"),
  );
  const data = join(root, "data");
  const connections: SQL[] = [];
  const managers: Manager[] = [];
  let startupAttempted = false;
  try {
    await command([
      binaries.initdb!,
      "-D",
      data,
      "-U",
      "auth_test",
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
    const connect = () => {
      const client = new SQL({
        adapter: "postgres",
        hostname: "127.0.0.1",
        port,
        username: "auth_test",
        password: "",
        database: "postgres",
        max: 1,
        connectionTimeout: 5,
        idleTimeout: 0,
        tls: false,
      });
      connections.push(client);
      return client;
    };
    const owner = connect();
    const observer = connect();
    const a = connect();
    const b = connect();
    const [version] = await observer`SELECT version() AS version`;
    console.info(`Real Auth PostgreSQL: ${version.version}`);
    for (const client of connections) {
      await client`SET statement_timeout = '8s'`;
    }
    const migration = await readFile(
      new URL("../migrations/pg/0000_auth_sessions.sql", import.meta.url),
      "utf8",
    );
    await owner.unsafe(migration);
    const store = postgresSessionStore(drizzle(a));
    const make = (client: SQL, idle = 30_000) => {
      const persistence = postgresSessionStore(drizzle(client));
      const manager = createManagedSessions({
        realmId: "postgres-test",
        login: {
          capabilities: { authenticatedAt: true, assurance: ["password"] },
          async verify() {
            return {
              status: "verified" as const,
              subjectId: "subject",
              session: {
                expiresAt: Date.now() + 60_000,
                authenticatedAt: Date.now(),
                assurance: ["password"],
              },
            };
          },
        },
        store: persistence,
        lifetime: { idle, absolute: 60_000, renewAfter: 20 },
        subjectActive: async () => true,
      });
      managers.push(manager);
      return manager;
    };
    const first = make(a);
    const second = make(b);
    const short = make(a, 1_500);
    // Capture backend IDs before launching work: these max:1 pools cannot query while blocked.
    const ids = new Map<SQL, number>();
    for (const client of [a, b]) {
      const [row] = await client`SELECT pg_backend_pid() AS pid`;
      ids.set(client, row.pid);
    }
    expect(new Set(ids.values()).size).toBe(2);
    const waitBlocked = async (clients: SQL[]) => {
      const pids = clients.map((client) => ids.get(client)!);
      await poll("distinct PostgreSQL row-lock waiters", async () => {
        const rows = await observer`
          SELECT pid FROM pg_stat_activity
          WHERE pid IN ${observer(pids)}
            AND wait_event_type = 'Lock' AND state = 'active'
        `;
        return rows.length === clients.length;
      });
    };
    await run({
      owner,
      observer,
      first,
      second,
      short,
      store,
      waitFor: async (clients) => waitBlocked(clients.map((name) => (name === "first" ? a : b))),
      readyToRenew: async (issued) =>
        poll("live renewal interval", async () => {
          const record = await store.read("postgres-test", issued.sessionId);
          return !!record && Date.now() >= record.renewedAt + record.renewAfterMs;
        }),
    });
  } finally {
    try {
      // Closing the lock owner first releases a transaction even when an assertion fails.
      for (const client of connections) await client.close({ timeout: 1 });
      await Promise.all(managers.map((manager) => manager.close()));
    } finally {
      try {
        if (startupAttempted) {
          const pidExists = await access(join(data, "postmaster.pid")).then(
            () => true,
            () => false,
          );
          if (pidExists) {
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
        }
      } finally {
        await rm(root, { recursive: true, force: true });
      }
    }
  }
}

function outcome<T>(promise: Promise<T>) {
  return promise.then(
    (value) => ({ status: "fulfilled" as const, value }),
    (reason: unknown) => ({ status: "rejected" as const, reason }),
  );
}

async function lock(owner: SQL, id: string) {
  await owner`BEGIN`;
  await owner`SELECT id FROM auth_sessions WHERE realm_id = 'postgres-test' AND id = ${id} FOR UPDATE`;
}

pgTest(
  "real PostgreSQL: two queued renewals have exactly one winner",
  async () => {
    await cluster(async ({ owner, first, second, store, waitFor, readyToRenew }) => {
      const issued = await first.issue(undefined);
      expect(await first.source.verify(issued.credential, context)).toMatchObject({
        status: "verified",
        subjectId: "subject",
        session: { assurance: ["password"] },
      });
      await readyToRenew(issued);
      await lock(owner, issued.sessionId);
      const a = outcome(first.renew(issued.credential));
      const b = outcome(second.renew(issued.credential));
      // Managers each own a separate max:1 Bun SQL pool.
      await waitFor(["first", "second"]);
      await owner`COMMIT`;
      const results = await Promise.all([a, b]);
      const winners = results.filter((result) => result.status === "fulfilled");
      const losers = results.filter((result) => result.status === "rejected");
      expect(winners).toHaveLength(1);
      expect(losers).toHaveLength(1);
      expect(losers[0]!.reason).toMatchObject({ code: "UNAUTHORIZED" });
      const renewed = winners[0]!.value;
      expect(renewed.sessionId).toBe(issued.sessionId);
      expect(renewed.credential !== issued.credential).toBe(true);
      await expect(first.source.verify(issued.credential, context)).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
      expect((await second.source.verify(renewed.credential, context)).status).toBe("verified");
      expect((await store.read("postgres-test", issued.sessionId))?.revision).toBe(2);
    });
  },
  30_000,
);

for (const order of ["revoke-first", "renew-first"] as const) {
  pgTest(
    `real PostgreSQL: ${order} lock queue ends revoked without resurrection`,
    async () => {
      await cluster(async ({ owner, first, second, waitFor, readyToRenew, store }) => {
        const issued = await first.issue(undefined);
        await readyToRenew(issued);
        await lock(owner, issued.sessionId);
        const leading =
          order === "revoke-first"
            ? outcome(first.revoke(issued.credential))
            : outcome(first.renew(issued.credential));
        await waitFor(["first"]);
        const trailing =
          order === "revoke-first"
            ? outcome(second.renew(issued.credential))
            : outcome(second.revoke(issued.credential));
        await waitFor(["first", "second"]);
        await owner`COMMIT`;
        const [leader, follower] = await Promise.all([leading, trailing]);
        expect(leader.status).toBe("fulfilled");
        if (order === "revoke-first") {
          expect(follower.status).toBe("rejected");
          if (follower.status !== "rejected")
            throw new Error("Renewal must refuse after revocation");
          expect(follower.reason).toMatchObject({ code: "UNAUTHORIZED" });
        } else {
          expect(follower.status).toBe("fulfilled");
          if (leader.status !== "fulfilled" || !leader.value)
            throw new Error("Renewal must succeed first");
          await expect(first.source.verify(leader.value.credential, context)).rejects.toMatchObject(
            { code: "UNAUTHORIZED" },
          );
          await expect(first.renew(leader.value.credential)).rejects.toMatchObject({
            code: "UNAUTHORIZED",
          });
        }
        await expect(first.source.verify(issued.credential, context)).rejects.toMatchObject({
          code: "UNAUTHORIZED",
        });
        expect((await store.read("postgres-test", issued.sessionId))?.revokedAt).toBeNumber();
      });
    },
    30_000,
  );
}

pgTest(
  "real PostgreSQL: renewal queued across idle expiry refuses mutation at database time",
  async () => {
    await cluster(async ({ owner, observer, short, store, waitFor, readyToRenew }) => {
      const issued = await short.issue(undefined);
      await readyToRenew(issued);
      const before = await store.read("postgres-test", issued.sessionId);
      await lock(owner, issued.sessionId);
      const pending = outcome(short.renew(issued.credential));
      await waitFor(["first"]);
      await poll("database clock crossing idle expiration", async () => {
        const [row] =
          await observer`SELECT floor(extract(epoch from clock_timestamp()) * 1000)::bigint AS now`;
        return Number(row.now) >= issued.expiresAt;
      });
      await owner`COMMIT`;
      const result = await pending;
      expect(result.status).toBe("rejected");
      if (result.status !== "rejected") throw new Error("Expired renewal must refuse");
      expect(result.reason).toMatchObject({ code: "UNAUTHORIZED" });
      expect(await store.read("postgres-test", issued.sessionId)).toEqual(before);
      await expect(short.source.verify(issued.credential, context)).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
    });
  },
  30_000,
);
