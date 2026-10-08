import { SQL } from "bun";
import { expect, test } from "bun:test";
import { access, mkdtemp, readFile, rm } from "node:fs/promises";
import { constants } from "node:fs";
import { createServer } from "node:net";
import { dirname, join } from "node:path";
import { drizzle } from "drizzle-orm/bun-sql";
import { createPostgresAuditRepository, auditPostgresSchema } from "../src/postgres";
import type { AuditEvent } from "../src/contracts";
import { createAuditService } from "../src/service";

const required = process.env.LENSO_REQUIRE_POSTGRES === "1";
const binaryNames = ["postgres", "initdb", "pg_ctl"] as const;
const binaries: Record<string, string> = {};
for (const name of binaryNames) {
  for (const path of [Bun.which(name), `/opt/homebrew/bin/${name}`]) {
    if (!path) continue;
    try {
      await access(path, constants.X_OK);
      binaries[name] = path;
      break;
    } catch {}
  }
}
const missing = binaryNames.filter((name) => !binaries[name]);
if (missing.length && required)
  throw new Error(`Required PostgreSQL binaries unavailable: ${missing.join(", ")}`);
if (missing.length)
  console.warn(
    `Skipping real audit PostgreSQL tests; unavailable: ${missing.join(", ")}. Set LENSO_REQUIRE_POSTGRES=1 to require them.`,
  );
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
  if (exit !== 0) throw new Error(`${args[0]} failed (${exit}): ${stdout}${stderr}`);
  return stdout.trim();
}

async function freePort() {
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

const uuid = (n: number) => `00000000-0000-0000-0000-${n.toString().padStart(12, "0")}`;
function event(
  id: number,
  scopeId = "scope",
  tenantId: string | null = null,
  recordedAt = 10,
): AuditEvent {
  return {
    id: uuid(id),
    occurredAt: 1,
    recordedAt,
    scope: { tenantId, scopeId },
    actor: { kind: "system", systemId: "worker" },
    action: "update",
    target: { type: "document", id: `doc-${id}` },
    result: "success",
    reasonCode: "completed",
    correlationId: "trace",
    summary: { count: id },
  };
}

pgTest(
  "real PostgreSQL audit repository persists, deduplicates concurrently, scopes and pages",
  async () => {
    const root = await mkdtemp(
      join(process.env.DELTA_SCRATCH_DIR ?? dirname(import.meta.dir), ".audit-pg-"),
    );
    const data = join(root, "data");
    const connections: SQL[] = [];
    let startupAttempted = false;
    try {
      await command([
        binaries.initdb!,
        "-D",
        data,
        "-U",
        "audit_test",
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
        `-h 127.0.0.1 -p ${port} -k ''`,
        "start",
      ]);
      const connect = () => {
        const sql = new SQL({
          adapter: "postgres",
          hostname: "127.0.0.1",
          port,
          username: "audit_test",
          password: "",
          database: "postgres",
          max: 1,
          connectionTimeout: 5,
          idleTimeout: 0,
          tls: false,
        });
        connections.push(sql);
        return sql;
      };
      const owner = connect();
      const firstClient = connect();
      const secondClient = connect();
      const observer = connect();
      await owner.unsafe(
        await readFile(new URL("../migrations/pg/0000_audit.sql", import.meta.url), "utf8"),
      );
      const firstRepo = createPostgresAuditRepository(
        drizzle(firstClient, { schema: auditPostgresSchema }),
      );
      const secondRepo = createPostgresAuditRepository(
        drizzle(secondClient, { schema: auditPostgresSchema }),
      );
      const first = event(1);
      const attempts = await Promise.all([
        firstRepo.insert(first),
        secondRepo.insert(first),
        firstRepo.insert(first),
      ]);
      expect(attempts.sort()).toEqual(["duplicate", "duplicate", "inserted"]);
      expect(await firstRepo.insert({ ...first, action: "delete" })).toBe("conflict");
      expect(await secondRepo.get(first.scope, first.id)).toEqual(first);
      expect(await firstRepo.get({ tenantId: null, scopeId: "other" }, first.id)).toBeNull();
      expect(await firstRepo.insert(event(1, "other"))).toBe("inserted");
      expect(await firstRepo.insert(event(1, "scope", "tenant"))).toBe("inserted");
      expect(await firstRepo.get({ tenantId: "tenant", scopeId: "scope" }, first.id)).toEqual(
        event(1, "scope", "tenant"),
      );
      const tiedA = event(2, "scope", null, 30);
      const tiedB = event(3, "scope", null, 30);
      await firstRepo.insert(tiedA);
      await secondRepo.insert(tiedB);
      expect(
        (await firstRepo.list({ scope: first.scope, limit: 2 })).map((item) => item.id),
      ).toEqual([tiedB.id, tiedA.id]);
      expect(
        (
          await firstRepo.list({
            scope: first.scope,
            limit: 2,
            cursor: { recordedAt: 30, id: tiedA.id },
          })
        ).map((item) => item.id),
      ).toEqual([first.id]);
      expect(
        await firstRepo.list({
          scope: first.scope,
          limit: 5,
          action: "update",
          target: tiedA.target,
          result: "success",
          correlationId: "trace",
          recordedFrom: 30,
          recordedTo: 30,
        }),
      ).toEqual([tiedA]);
      const stored = await observer`SELECT event_json FROM lenso_audit_events
      WHERE tenant_key = 'null' AND scope_id = 'scope' AND id = ${first.id}`;
      expect(stored).toHaveLength(1);
      expect(JSON.parse(stored[0].event_json)).toEqual(first);
      expect((await owner`SHOW fsync`)[0].fsync).toBe("on");
      expect((await firstClient`SHOW synchronous_commit`)[0].synchronous_commit).toBe("on");
      const system = Object.freeze({});
      const strict = createAuditService({
        repository: createPostgresAuditRepository(
          drizzle(firstClient, { schema: auditPostgresSchema }),
          { durableIntents: true },
        ),
        authority: {
          async resolve(principal: object) {
            if (principal !== system) throw new Error("not authorized");
            return { kind: "system", systemId: "worker" };
          },
        },
      });
      const intent = {
        id: uuid(10),
        occurredAt: 1,
        scope: first.scope,
        action: "strict.test",
        target: { type: "resource", id: "fixture-only" },
        result: "intent" as const,
        reasonCode: "requested",
      };
      const prepared = await strict.prepare(intent, system);
      expect(prepared.status).toBe("ready");
      // An independent connection sees the committed intent before any fixture effect.
      expect((await secondRepo.get(first.scope, intent.id))?.result).toBe("intent");
      expect(await strict.prepare(intent, system)).toEqual({
        status: "already-recorded",
        intentId: intent.id,
      });
      if (prepared.status !== "ready") throw new Error("Expected ready receipt");
      const outcome = await strict.complete(prepared.receipt, {
        id: uuid(11),
        occurredAt: 2,
        result: "success",
        reasonCode: "fixture-completed",
      });
      expect(await secondRepo.get(first.scope, outcome.id)).toEqual(outcome);
    } finally {
      try {
        await Promise.allSettled(connections.map((connection) => connection.close({ timeout: 1 })));
      } finally {
        try {
          if (
            startupAttempted &&
            (await access(join(data, "postmaster.pid")).then(
              () => true,
              () => false,
            ))
          ) {
            await command([binaries.pg_ctl!, "-D", data, "-w", "-t", "10", "-m", "fast", "stop"]);
          }
        } finally {
          await rm(root, { recursive: true, force: true });
        }
      }
    }
  },
  30_000,
);
