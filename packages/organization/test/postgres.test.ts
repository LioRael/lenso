import assert from "node:assert/strict";
import { SQL } from "bun";
import { test } from "bun:test";
import { constants } from "node:fs";
import { access as fileAccess, mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { drizzle } from "drizzle-orm/bun-sql";
import { postgresOrganizationStore } from "../src/drizzle/pg";
import { createOrganizationService } from "../src/service";
import { actors, backendContract, poll } from "./backend-contract";

const names = ["postgres", "initdb", "pg_ctl"] as const;
const binaries: Record<string, string> = {};
for (const name of names) {
  for (const path of [Bun.which(name), `/opt/homebrew/bin/${name}`]) {
    if (!path) continue;
    if (
      await fileAccess(path, constants.X_OK).then(
        () => true,
        () => false,
      )
    ) {
      binaries[name] = path;
      break;
    }
  }
}
const missing = names.filter((name) => !binaries[name]);
if (missing.length && process.env.LENSO_REQUIRE_POSTGRES === "1") {
  throw new Error(`Required PostgreSQL integration binaries unavailable: ${missing.join(", ")}`);
}
if (missing.length) {
  console.warn(
    `Skipping real organization PostgreSQL tests: missing ${missing.join(", ")}. Set LENSO_REQUIRE_POSTGRES=1 to require them.`,
  );
}
const pgTest = missing.length ? test.skip : test;

async function command(args: string[]) {
  const child = Bun.spawn(args, {
    env: { PATH: process.env.PATH ?? "/usr/bin:/bin", LC_ALL: "C" },
    stdout: "pipe",
    stderr: "pipe",
  });
  const timer = setTimeout(() => child.kill(), 15_000);
  try {
    const [exit, stdout, stderr] = await Promise.all([
      child.exited,
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
    ]);
    if (exit !== 0) throw new Error(`${args[0]} failed (${exit}): ${stdout}${stderr}`);
  } finally {
    clearTimeout(timer);
  }
}

async function freePort() {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  const port = address.port;
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
  return port;
}

async function cluster(
  run: (fixture: {
    owner: SQL;
    observer: SQL;
    first: SQL;
    second: SQL;
    firstPid: number;
  }) => Promise<void>,
) {
  const root = await mkdtemp(join(process.env.DELTA_SCRATCH_DIR ?? tmpdir(), "organization-pg-"));
  const data = join(root, "data");
  const clients: SQL[] = [];
  let startupAttempted = false;
  try {
    await command([
      binaries.initdb!,
      "-D",
      data,
      "-U",
      "organization_test",
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
        username: "organization_test",
        password: "",
        database: "postgres",
        max: 1,
        connectionTimeout: 5,
        idleTimeout: 0,
        tls: false,
      });
      clients.push(client);
      return client;
    };
    const owner = connect();
    const observer = connect();
    const first = connect();
    const second = connect();
    for (const client of clients) await client`SET statement_timeout = '8s'`;
    const [version] = await observer`SELECT version() AS version`;
    console.info(`Real organization PostgreSQL: ${version.version}`);
    await owner.unsafe(
      await readFile(new URL("../migrations/pg/0000_organizations.sql", import.meta.url), "utf8"),
    );
    const [a] = await first`SELECT pg_backend_pid() AS pid`;
    const [b] = await second`SELECT pg_backend_pid() AS pid`;
    assert.notEqual(a.pid, b.pid);
    await run({ owner, observer, first, second, firstPid: a.pid });
  } finally {
    try {
      // Close the blocker first so failed assertions cannot retain its row lock.
      for (const client of clients) await client.close({ timeout: 1 });
    } finally {
      try {
        if (
          startupAttempted &&
          (await fileAccess(join(data, "postmaster.pid")).then(
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

pgTest(
  "real PostgreSQL organization backend contract",
  async () => {
    await cluster(async ({ first, second, observer }) => {
      await backendContract(
        [postgresOrganizationStore(drizzle(first)), postgresOrganizationStore(drizzle(second))],
        async (id) => (await observer`SELECT * FROM organizations WHERE id = ${id}`)[0],
      );
    });
  },
  30_000,
);

pgTest(
  "real PostgreSQL invitation acceptance queued across expiry cannot commit",
  async () => {
    await cluster(async ({ owner, observer, first, firstPid }) => {
      const { actor, subject, access } = actors();
      const a = actor("owner");
      const b = actor("invitee");
      const store = postgresOrganizationStore(drizzle(first));
      const service = createOrganizationService({
        store,
        access,
        config: { invitationLifetimeMs: 1500 },
      });
      const org = (await service.createOrganization(a, { name: "expiry lock queue" })).value;
      const issued = await service.createInvitation(a, {
        organizationId: org.id,
        target: subject(b),
      });
      const before = (await store.read(org.id))!.state;
      await owner`BEGIN`;
      await owner`SELECT id FROM organizations WHERE id = ${org.id} FOR UPDATE`;
      const acceptance = service
        .acceptInvitation(b, {
          organizationId: org.id,
          invitationId: issued.value.invitation.id,
          token: issued.value.token,
        })
        .then(
          (value) => ({ status: "fulfilled" as const, value }),
          (reason: unknown) => ({ status: "rejected" as const, reason }),
        );
      await poll("acceptance waiting on PostgreSQL row lock", async () => {
        const rows = await observer`
        SELECT pid FROM pg_stat_activity
        WHERE pid = ${firstPid} AND state = 'active' AND wait_event_type = 'Lock'
      `;
        return rows.length === 1;
      });
      await poll("database clock crossing invitation expiry", async () => {
        const [row] = await observer`
        SELECT floor(extract(epoch from clock_timestamp()) * 1000)::bigint AS now
      `;
        return Number(row.now) >= issued.value.invitation.expiresAt;
      });
      // No version change: only the post-lock storage clock can refuse this CAS.
      await owner`COMMIT`;
      const result = await acceptance;
      assert.equal(result.status, "rejected");
      if (result.status !== "rejected") throw new Error("Expired acceptance committed");
      assert.ok(result.reason && typeof result.reason === "object" && "code" in result.reason);
      assert.equal(result.reason.code, "INVITATION_EXPIRED");
      assert.deepEqual((await store.read(org.id))!.state, before);
      assert.equal(
        JSON.stringify(
          (await observer`SELECT * FROM organizations WHERE id = ${org.id}`)[0],
        ).includes(issued.value.token),
        false,
      );
    });
  },
  30_000,
);
