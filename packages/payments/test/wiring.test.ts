import { expect, test } from "bun:test";
import { audience, createAuth, defineSource, realm, type Actor } from "@lenso/auth";
import { valuesSource } from "@lenso/core/config";
import { defineApp, definePlugin, startApp } from "@lenso/core";
import { createPayments } from "../src";
import { paymentsAuthorization } from "../src/auth";
import { paymentsConfig } from "../src/config";
import { createPaymentsPlugin } from "../src/plugin";
import { createPaymentsManage } from "../src/manage";
import type {
  PaymentAction,
  PaymentResource,
  PaymentsProvider,
  PaymentsStore,
} from "../src/contracts";

function providers() {
  const store: PaymentsStore = {
    async insert() {
      return true;
    },
    async get() {
      return null;
    },
    async getByOrder() {
      return null;
    },
    async compareAndSet() {
      return true;
    },
    async due() {
      return [];
    },
    async receive() {
      return true;
    },
    async inbox() {
      return [];
    },
    async finishEvent() {},
    async deferEvent() {},
  };
  const unavailable = async (): Promise<never> => {
    throw new Error("fixture unavailable");
  };
  const provider: PaymentsProvider = {
    accountId: "acct_fixture",
    live: false,
    replayWindowMs: 23 * 3_600_000,
    validateAmount() {},
    validateRefundAmount() {},
    createPayment: unavailable,
    getPayment: unavailable,
    findPayment: unavailable,
    createRefund: unavailable,
    getRefund: unavailable,
    findRefund: unavailable,
    clientSecret: unavailable,
    verifyWebhook: unavailable,
  };
  return { store, provider };
}

test("Auth validates provenance, audience, reverified credentials and resource tenant for every action", async () => {
  let revoked = false;
  const source = defineSource<string>({
    async verify(subjectId) {
      return revoked ? { status: "rejected" } : { status: "verified", subjectId, kind: "service" };
    },
  });
  const auth = createAuth(realm("payments-fixture", source));
  const otherAuth = createAuth(realm("payments-fixture", source));
  try {
    const access = auth
      .for(audience("payments"))
      .memberships<PaymentResource, string>(async (subject) => subject.subjectId);
    const actions: PaymentAction[] = ["create", "read", "client-secret", "refund", "results"];
    const authorize = paymentsAuthorization(access, {
      create: ({ membership, resource }) => membership === resource.tenantId,
      read: ({ membership, resource }) => membership === resource.tenantId,
      "client-secret": ({ membership, resource }) => membership === resource.tenantId,
      refund: ({ membership, resource }) => membership === resource.tenantId,
      results: ({ membership, resource }) => membership === resource.tenantId,
    });
    const actor = await access.required("tenant-a");
    const record: PaymentResource = {
      paymentId: "fixture",
      tenantId: "tenant-a",
      orderId: "order-a",
      amount: 1000,
      currency: "usd",
      accountId: "acct_fixture",
      live: false,
    };
    const wrongTenant = await access.required("tenant-b");
    const wrongAudience = await auth.for(audience("other")).required("tenant-a");
    const wrongInstance = await otherAuth.for(audience("payments")).required("tenant-a");
    for (const action of actions) {
      await authorize(actor, action, record);
      await expect(authorize(wrongTenant, action, record)).rejects.toMatchObject({
        code: "FORBIDDEN",
      });
      for (const forged of [{ ...actor }, wrongAudience, wrongInstance]) {
        await expect(
          authorize(forged as Actor<"payments-fixture", string, "payments">, action, record),
        ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
      }
    }
    revoked = true;
    await expect(authorize(actor, "refund", record)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
  } finally {
    await otherAuth.close();
    await auth.close();
  }
});

test("plugin binds exact dependencies; configured runtime and Manage remain explicit and read-only", async () => {
  const services = providers();
  const store = definePlugin({ id: "fixture-store", setup: () => services.store });
  const provider = definePlugin({ id: "fixture-provider", setup: () => services.provider });
  const authorization = definePlugin({
    id: "fixture-authorization",
    setup:
      () =>
      async (_actor: { tenantId: string }, _action: PaymentAction, _record: PaymentResource) => {},
  });
  const plugin = createPaymentsPlugin({
    id: "fixture-payments",
    store,
    provider,
    authorization,
    config: {
      contract: paymentsConfig,
      sources: [valuesSource({ maxRefunds: 2 }, { id: "fixture-config" })],
    },
  });
  expect(plugin.requires).toEqual([store, provider, authorization]);
  const manage = createPaymentsManage({ id: "fixture-payments-status", payments: plugin });
  expect(manage.plugin.requires?.[0]).toBe(plugin);
  expect(manage.operations.map((operation) => operation.method)).toEqual(["status"]);
  expect(manage.operations[0].context).toBe(true);
  const app = await startApp(
    defineApp({ plugins: [store, provider, authorization, plugin, manage.plugin] }),
  );
  try {
    const runtime = app.get(plugin);
    expect(typeof runtime.payments.create).toBe("function");
    const method = app.get(manage.plugin).status;
    await expect(
      method({ paymentId: "not-existing" }, { actor: { tenantId: "a" } }),
    ).rejects.toMatchObject({ code: "not-found" });
  } finally {
    await app.stop();
  }
  const otherStore = definePlugin({ id: store.id, setup: () => services.store });
  await expect(
    startApp(defineApp({ plugins: [otherStore, provider, authorization, plugin] })),
  ).rejects.toBeDefined();
  expect(createPayments({ ...services, authorize: async () => {} }).payments).toBeDefined();
});

test("ordinary core import bundles without external optional integration imports", async () => {
  const build = await Bun.build({
    entrypoints: [new URL("../src/index.ts", import.meta.url).pathname],
    target: "bun",
    packages: "external",
  });
  expect(build.success).toBe(true);
  expect(build.outputs).toHaveLength(1);
  const output = await build.outputs[0].text();
  expect(output).not.toMatch(/from ["'](?:stripe|zod|drizzle-orm|@lenso\/|@opentelemetry\/)/);
});
