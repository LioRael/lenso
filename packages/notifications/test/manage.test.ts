import { expect, test } from "bun:test";
import { defineApp, startApp } from "@lenso/core";
import { definePlugin } from "@lenso/core/plugin";
import { createManageAdapter } from "@lenso/manage";
import { createNotificationsManage } from "../src/manage";
import { notificationQueryInput } from "../src/auth";
import { authFixture } from "./auth.test";

test("management is disabled by default and incomplete enabled configuration fails closed", () => {
  expect(createNotificationsManage()).toBeUndefined();
  expect(createNotificationsManage({ enabled: false })).toBeUndefined();
  expect(() => createNotificationsManage({ enabled: true } as never)).toThrow(TypeError);
});

test("exact plugins, shared schemas and trusted second actor parameter govern explicit Manage invocation", async () => {
  const f = authFixture();
  const notifications = definePlugin({ id: "notifications", setup: () => f.service });
  const authentication = definePlugin({ id: "authentication", setup: () => f.access });
  const bundle = createNotificationsManage({
    enabled: true,
    notifications,
    authentication,
    tenantFor: f.tenantFor,
    managePolicy: f.managePolicy,
    requeue: f.requeue,
  })!;
  expect(bundle.plugin.requires).toEqual([notifications, authentication]);
  expect(bundle.operations.every((op) => op.plugin === bundle.plugin && op.context === true)).toBe(
    true,
  );
  expect(bundle.operations.find((op) => op.method === "query")!.input).toBe(notificationQueryInput);
  expect(bundle.manage.operations[0]).toBe(bundle.operations[0]);
  expect(bundle.plugin.source).toEqual({
    file: "packages/notifications/src/manage.ts",
    export: "createNotificationsManage",
  });
  const running = await startApp(
    defineApp({ plugins: [notifications, authentication, bundle.plugin] }),
  );
  try {
    const row = await f.create();
    await f.service.deliver(row.id);
    let actor = await f.access.required("admin");
    const adapter = createManageAdapter({
      running,
      plugins: [notifications, authentication, bundle.plugin],
      operations: bundle.operations,
      binding: () => ({ context: actor }),
      canList: () => true,
    });
    expect(await adapter.invoke(bundle.plugin.id, "retry", { id: row.id })).toEqual({
      queued: true,
    });
    expect(f.sends()).toBe(1);
    expect(f.queued).toEqual([row.id]);
    const status = await adapter.invoke(bundle.plugin.id, "adminQuery", { id: row.id });
    expect(JSON.stringify(status)).not.toContain("private");
    await expect(
      adapter.invoke(bundle.plugin.id, "retry", { id: row.id, actor }),
    ).rejects.toMatchObject({ diagnostic: { code: "invalid-input" } });
    const unbound = createManageAdapter({
      running,
      plugins: [notifications, authentication, bundle.plugin],
      operations: bundle.operations,
      binding: () => ({ context: undefined as never }),
      canList: () => true,
    });
    await expect(unbound.invoke(bundle.plugin.id, "query", { id: row.id })).rejects.toBeDefined();
    actor = await f.access.required("alice");
    await expect(adapter.invoke(bundle.plugin.id, "retry", { id: row.id })).rejects.toBeDefined();
    actor = { ...actor };
    await expect(adapter.invoke(bundle.plugin.id, "query", { id: row.id })).rejects.toBeDefined();
    actor = await f.access.required("admin");
    f.revoke();
    await expect(
      adapter.invoke(bundle.plugin.id, "adminQuery", { id: row.id }),
    ).rejects.toBeDefined();
  } finally {
    await running.stop();
    await f.auth.close();
  }
});

test("same-id replacement does not satisfy exact authentication dependency", async () => {
  const f = authFixture();
  try {
    const notifications = definePlugin({ id: "notifications", setup: () => f.service });
    const authentication = definePlugin({ id: "authentication", setup: () => f.access });
    const impostor = definePlugin({ id: "authentication", setup: () => f.access });
    const bundle = createNotificationsManage({
      enabled: true,
      notifications,
      authentication,
      tenantFor: f.tenantFor,
      managePolicy: f.managePolicy,
      requeue: f.requeue,
    })!;
    await expect(
      startApp(defineApp({ plugins: [notifications, impostor, bundle.plugin] })),
    ).rejects.toMatchObject({ diagnostics: [{ code: "missing-dependency" }] });
  } finally {
    await f.auth.close();
  }
});
