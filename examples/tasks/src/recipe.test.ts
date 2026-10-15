import { expect, test } from "bun:test";
import { createAuth, defineSource, realm, type SubjectRef } from "@lenso/auth";
import { createAuthPlugin } from "@lenso/auth/plugin";
import { defineApp, definePlugin, startApp } from "@lenso/core";
import type { JobStatus } from "@lenso/tasks";
import { createTaskRecipe } from "./recipe";
import { localRecipeConfig } from "./recipe-demo";
import type { TaskServiceResources } from "./authorized-service";
import { createReportTask } from "./task";

function fixture() {
  let credential: string | null = "alice-evidence";
  let authSetups = 0;
  let authCloses = 0;
  let resourceSetups = 0;
  let resourceCloses = 0;
  let queueEffects = 0;
  let reportReads = 0;
  const owners = new Map<string, SubjectRef>();
  const jobReports = new Map<string, string>();
  const statuses = new Map<string, JobStatus>();
  const auth = createAuthPlugin({
    id: "application-auth",
    setup() {
      authSetups++;
      const existing = createAuth(
        realm(
          "application-realm",
          defineSource<string | null>({
            async verify(evidence) {
              if (evidence === null) return { status: "absent" };
              const subjectId =
                evidence === "alice-evidence"
                  ? "alice"
                  : evidence === "bob-evidence"
                    ? "bob"
                    : null;
              return subjectId ? { status: "verified", subjectId } : { status: "rejected" };
            },
          }),
        ),
      );
      return {
        for: existing.for,
        async close() {
          authCloses++;
          await existing.close();
        },
      };
    },
  });
  const reports = {
    async generate(input: { reportId: string }) {
      return { reportId: input.reportId, sum: 6, count: 3 };
    },
    async get() {
      reportReads++;
      return { sum: 6, count: 3 };
    },
  };
  const task = createReportTask(reports);
  const supplied: TaskServiceResources = {
    reports,
    task,
    queue: {
      async enqueue(actualTask) {
        expect(Object.is(actualTask, task)).toBe(true);
        queueEffects++;
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
        return jobId;
      },
      async get(jobId) {
        queueEffects++;
        return statuses.get(jobId) ?? null;
      },
      async cancel() {
        queueEffects++;
        return "cancelled";
      },
      async retry() {
        queueEffects++;
        return false;
      },
    },
    ownership: {
      async claim(reportId, owner) {
        if (!owners.has(reportId)) owners.set(reportId, owner);
        return owners.get(reportId)!;
      },
      async report(reportId) {
        return owners.get(reportId) ?? null;
      },
      async job(jobId) {
        return owners.get(jobReports.get(jobId) ?? "") ?? null;
      },
      async record(jobId, reportId) {
        jobReports.set(jobId, reportId);
      },
    },
  };
  const resources = definePlugin({
    id: "application-resources",
    setup(context) {
      resourceSetups++;
      context.onCleanup(() => {
        resourceCloses++;
      });
      return supplied;
    },
  });
  const recipe = createTaskRecipe({
    id: "report-jobs",
    auth,
    resources,
    evidence: () => credential,
  });
  return {
    auth,
    resources,
    supplied,
    recipe,
    owners,
    setCredential: (value: string | null) => {
      credential = value;
    },
    counts: () => ({
      authSetups,
      authCloses,
      resourceSetups,
      resourceCloses,
      queueEffects,
      reportReads,
    }),
  };
}

test("recipe uses the exact supplied Auth, task and resource plugin instances", async () => {
  const f = fixture();
  expect(f.recipe.plugin.requires).toEqual([f.auth, f.resources]);
  expect(f.recipe.operations.every((operation) => operation.plugin === f.recipe.plugin)).toBe(true);
  expect(f.counts().authSetups).toBe(0);
  const app = await startApp(defineApp({ plugins: [f.auth, f.resources, f.recipe.plugin] }));
  try {
    expect(app.get(f.resources)).toBe(f.supplied);
    const service = app.get(f.recipe.plugin);
    const submitted = await service.submit({ reportId: "identity", rows: [1, 2, 3] });
    expect(f.owners.get("identity")).toEqual({ realmId: "application-realm", subjectId: "alice" });
    expect(await service.query(submitted)).toMatchObject({ state: "pending" });
    expect(await service.report({ reportId: "identity" })).toEqual({ sum: 6, count: 3 });
    expect(f.counts()).toEqual({
      authSetups: 1,
      authCloses: 0,
      resourceSetups: 1,
      resourceCloses: 0,
      queueEffects: 2,
      reportReads: 1,
    });
  } finally {
    await app.stop();
  }
  expect(f.counts().authCloses).toBe(1);
  expect(f.counts().resourceCloses).toBe(1);
});

test("recipe rejects cross-owner and missing credentials before queue/report effects", async () => {
  const f = fixture();
  const app = await startApp(defineApp({ plugins: [f.auth, f.resources, f.recipe.plugin] }));
  try {
    const service = app.get(f.recipe.plugin);
    const submitted = await service.submit({ reportId: "owned", rows: [1] });
    const before = f.counts();
    f.setCredential("bob-evidence");
    for (const method of ["query", "cancel", "retry"] as const)
      await expect(service[method](submitted)).rejects.toMatchObject({ code: "FORBIDDEN" });
    await expect(service.report({ reportId: "owned" })).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
    await expect(service.submit({ reportId: "owned", rows: [99] })).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
    for (const credential of [null, "invalid-evidence"]) {
      f.setCredential(credential);
      await expect(service.query(submitted)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
      await expect(service.submit({ reportId: "unclaimed", rows: [] })).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
    }
    expect(f.owners.has("unclaimed")).toBe(false);
    expect(f.counts()).toEqual(before);
  } finally {
    await app.stop();
  }
});

test("missing supplied dependencies fail before any setup", async () => {
  const f = fixture();
  await expect(
    startApp(defineApp({ plugins: [f.resources, f.recipe.plugin] })),
  ).rejects.toMatchObject({
    diagnostics: [{ code: "missing-dependency", dependencyId: "application-auth" }],
  });
  expect(f.counts().resourceSetups).toBe(0);
  expect(f.counts().authSetups).toBe(0);
});

test("demo only accepts explicit local disposable database configuration without leaking its value", () => {
  const local = "postgresql://127.0.0.1:55483/lenso_recipe_tasks";
  expect(localRecipeConfig(local).connectionString).toBe(local);
  for (const invalid of [
    undefined,
    "malformed-secret",
    "postgres://user:secret@remote/lenso_recipe_tasks",
    "postgres://localhost/production",
    `${local}?host=remote`,
  ]) {
    expect(() => localRecipeConfig(invalid)).toThrow("Set TASK_RECIPE_DATABASE_URL");
  }
});
