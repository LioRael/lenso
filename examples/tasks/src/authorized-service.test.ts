import { describe, expect, test } from "bun:test";
import { defineSource, type SubjectRef } from "@lenso/auth";
import type { JobState, JobStatus } from "@lenso/tasks";
import { defineApp } from "@lenso/core";
import { defineOperation, invoke } from "@lenso/cli";
import { createAuthorizedTaskService } from "./authorized-service";
import { jobInput, submitInput } from "./contracts";
import type { OwnershipStore } from "./ownership";
import { createTasksPlugin } from "./plugin";
import { createReportTask } from "./task";

function fixture() {
  const owners = new Map<string, SubjectRef>();
  const jobs = new Map<string, string>();
  const statuses = new Map<string, JobStatus>();
  let credential: string | null = "alice-session";
  let revoked = false;
  let effects = 0;
  let closed = 0;
  const source = defineSource<string | null>({
    async verify(evidence) {
      if (evidence === null) return { status: "absent" };
      if (revoked) return { status: "rejected" };
      const subjectId =
        evidence === "alice-session" ? "alice" : evidence === "bob-session" ? "bob" : null;
      return subjectId ? { status: "verified", subjectId } : { status: "rejected" };
    },
  });
  const ownership: OwnershipStore = {
    async claim(reportId, owner) {
      if (!owners.has(reportId)) owners.set(reportId, owner);
      return owners.get(reportId)!;
    },
    async report(reportId) {
      return owners.get(reportId) ?? null;
    },
    async job(jobId) {
      return owners.get(jobs.get(jobId) ?? "") ?? null;
    },
    async record(jobId, reportId) {
      jobs.set(jobId, reportId);
    },
  };
  const dedup = new Map<string, string>();
  const task = createReportTask({
    async generate(input) {
      return { reportId: input.reportId, sum: 3, count: 2 };
    },
    async get() {
      return { sum: 3, count: 2 };
    },
  });
  const resources: Omit<
    Parameters<typeof createAuthorizedTaskService>[0],
    "source" | "evidence"
  > & { close: () => Promise<void> } = {
    task,
    ownership,
    reports: {
      async get() {
        effects++;
        return { sum: 3, count: 2 };
      },
    },
    queue: {
      async enqueue(_task, _input, options) {
        effects++;
        if (options?.deduplicationKey && dedup.has(options.deduplicationKey))
          return dedup.get(options.deduplicationKey)!;
        const jobId = crypto.randomUUID();
        statuses.set(jobId, {
          jobId,
          task: task.name,
          state: "pending",
          attempt: 0,
          maxAttempts: 3,
          cancelRequested: false,
          result: null,
          error: null,
        });
        if (options?.deduplicationKey) dedup.set(options.deduplicationKey, jobId);
        return jobId;
      },
      async get(id) {
        effects++;
        return statuses.get(id) ?? null;
      },
      async cancel(id) {
        effects++;
        const status = statuses.get(id);
        if (!status) return "missing";
        if (status.state === "running") {
          statuses.set(id, { ...status, cancelRequested: true });
          return "requested";
        }
        if (status.state === "pending") {
          statuses.set(id, { ...status, state: "cancelled" });
          return "cancelled";
        }
        return "terminal";
      },
      async retry(id) {
        effects++;
        const status = statuses.get(id);
        if (status?.state !== "failed") return false;
        statuses.set(id, { ...status, state: "pending", maxAttempts: status.attempt + 1 });
        return true;
      },
    },
    async close() {
      closed++;
    },
  };
  const service = createAuthorizedTaskService({ ...resources, source, evidence: () => credential });
  return {
    service,
    resources,
    source,
    owners,
    statuses,
    setCredential(value: string | null) {
      credential = value;
    },
    revoke() {
      revoked = true;
    },
    evidence: () => credential,
    effects: () => effects,
    closed: () => closed,
    state(id: string, state: JobState, attempt = 0) {
      statuses.set(id, { ...statuses.get(id)!, state, attempt });
    },
  };
}

describe("authorized tasks", () => {
  test("enforce revalidates a session revoked during durable owner lookup", async () => {
    const f = fixture();
    const job = await f.service.submit({ reportId: "revoked", rows: [] });
    const lookup = f.resources.ownership.job;
    f.resources.ownership.job = async (jobId) => {
      const owner = await lookup(jobId);
      f.revoke();
      return owner;
    };
    const before = f.effects();
    await expect(f.service.cancel(job)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    expect(f.effects()).toBe(before);
    await f.service.close();
  });

  test("fresh CLI instances share ownership, deny cross-owner calls before queue effects", async () => {
    const f = fixture();
    const plugin = createTasksPlugin({
      connectAuth: async () => ({ source: f.source }),
      evidence: f.evidence,
      connectResources: async () => f.resources,
    });
    const app = {
      ...defineApp({ plugins: [plugin] }),
      operations: [
        defineOperation({
          plugin,
          method: "submit",
          input: submitInput,
          description: "Submit fixture.",
        }),
        defineOperation({
          plugin,
          method: "query",
          input: jobInput,
          description: "Query fixture.",
        }),
        defineOperation({
          plugin,
          method: "cancel",
          input: jobInput,
          description: "Cancel fixture.",
        }),
        defineOperation({
          plugin,
          method: "retry",
          input: jobInput,
          description: "Retry fixture.",
        }),
      ],
    };
    const { jobId } = (await invoke(app, "tasks", "submit", {
      reportId: "daily",
      rows: [1, 2],
    })) as { jobId: string };
    expect(await invoke(app, "tasks", "query", { jobId })).toMatchObject({ state: "pending" });
    f.setCredential("bob-session");
    const before = f.effects();
    for (const method of ["query", "cancel", "retry"]) {
      await expect(invoke(app, "tasks", method, { jobId })).rejects.toMatchObject({
        diagnostic: { code: "FORBIDDEN" },
      });
    }
    await expect(
      invoke(app, "tasks", "submit", { reportId: "daily", rows: [99] }),
    ).rejects.toMatchObject({ diagnostic: { code: "FORBIDDEN" } });
    expect(f.effects()).toBe(before);
    expect(f.closed()).toBe(6);
    await f.service.close();
  });

  test("cancel preserves all provider values and requested is not stopped", async () => {
    const f = fixture();
    const { jobId } = await f.service.submit({ reportId: "cancel", rows: [] });
    f.state(jobId, "running", 1);
    expect(await f.service.cancel({ jobId })).toBe("requested");
    expect(await f.service.query({ jobId })).toMatchObject({
      state: "running",
      cancelRequested: true,
    });
    f.state(jobId, "pending");
    expect(await f.service.cancel({ jobId })).toBe("cancelled");
    expect(await f.service.cancel({ jobId })).toBe("terminal");
    f.statuses.delete(jobId);
    expect(await f.service.cancel({ jobId })).toBe("missing");
    await expect(f.service.cancel({ jobId: crypto.randomUUID() })).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
    await f.service.close();
  });

  test("retry only final failure, preserves attempts and owner", async () => {
    const f = fixture();
    const { jobId } = await f.service.submit({ reportId: "retry", rows: [1], failUntilAttempt: 3 });
    expect(await f.service.retry({ jobId })).toBe(false);
    f.state(jobId, "failed", 3);
    expect(await f.service.retry({ jobId })).toBe(true);
    expect(await f.service.query({ jobId })).toMatchObject({
      state: "pending",
      attempt: 3,
      maxAttempts: 4,
    });
    f.setCredential("bob-session");
    await expect(f.service.retry({ jobId })).rejects.toMatchObject({ code: "FORBIDDEN" });
    await f.service.close();
  });

  test("no actor inputs, missing/revoked evidence denied, dedup scoped to business owner", async () => {
    const f = fixture();
    expect(
      submitInput.safeParse({ reportId: "x", rows: [], actor: { subjectId: "alice" } }).success,
    ).toBe(false);
    const input = { reportId: "x", rows: [], deduplicationKey: "same" };
    const a = await f.service.submit(input);
    expect(await f.service.submit(input)).toEqual(a);
    f.setCredential("bob-session");
    expect(await f.service.submit({ ...input, reportId: "y" })).not.toEqual(a);
    await expect(f.service.report({ reportId: "x" })).rejects.toMatchObject({ code: "FORBIDDEN" });
    f.setCredential(null);
    await expect(f.service.query(a)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    f.setCredential("alice-session");
    f.revoke();
    await expect(f.service.submit({ reportId: "reserved", rows: [] })).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    expect(f.owners.has("reserved")).toBe(false);
    await f.service.close();
  });
});
