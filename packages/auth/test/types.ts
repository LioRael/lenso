import { os } from "@orpc/server";
import { audience, createAuth, defineSource, realm, type ActorOf } from "../src/index";
import { bearerEvidence } from "../src/fetch";
import { optionalAuth, requiredAuth } from "../src/orpc";

declare const accountIdBrand: unique symbol;
type AccountId = string & { readonly [accountIdBrand]: true };

// Compile-only assertions. This function is never invoked by the test runner.
export async function checkTypes(accountId: AccountId) {
  const auth = createAuth(
    realm(
      "employees",
      defineSource({
        async verify(_token: string | null) {
          return { status: "verified", subjectId: accountId };
        },
      }),
    ),
  );
  const access = auth
    .for(audience("notes:read"))
    .memberships(async (_subject, resource: { tenantId: string }) =>
      resource.tenantId ? { role: "editor" as const } : null,
    );
  const actor = await access.required("token");
  const inferred: AccountId = actor.subjectId;
  const nullable = await access.optional(null);
  // @ts-expect-error Optional authentication must be narrowed before use.
  const unsafe: ActorOf<typeof access> = nullable;
  // @ts-expect-error Plain identity data cannot construct an actor.
  const forged: ActorOf<typeof access> = {
    realmId: "employees",
    subjectId: accountId,
    audience: "notes:read",
    kind: "user",
  };
  const write = auth.for(audience("notes:write"));
  // @ts-expect-error Actors are scoped to an exact operation audience.
  await write.enforce(actor, {}, () => true);
  await access.enforce(actor, { tenantId: "north" }, ({ membership }) => {
    const role: "editor" = membership.role;
    return role === "editor";
  });
  const base = os.$context<{ request: Request }>();
  base.use(optionalAuth(access, bearerEvidence)).handler(({ context }) => {
    // @ts-expect-error Optional middleware must retain the nullable actor type.
    const required: ActorOf<typeof access> = context.actor;
    void required;
    return context.actor?.subjectId;
  });
  base.use(requiredAuth(access, bearerEvidence)).handler(({ context }) => {
    const required: ActorOf<typeof access> = context.actor;
    return required.subjectId;
  });
  const prefilled = (context: { request: Request; actor: ActorOf<typeof access> }) =>
    bearerEvidence(context);
  // @ts-expect-error The actor key is reserved, not intersected into a nullable result.
  optionalAuth(access, prefilled);
  void inferred;
  void unsafe;
  void forged;
  await auth.close();
}
