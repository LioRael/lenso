import type { Access, Actor } from "@lenso/auth";
import type { AuditAuthority, AuditScope } from "./contracts";

export function createAuthAuditAuthority<R extends string, E, S extends string, A extends string>(
  access: Access<R, E, S, A>,
  policy: (input: {
    principal: Actor<R, S, A>;
    scope: AuditScope;
    operation: "append" | "query";
  }) => boolean | Promise<boolean>,
): AuditAuthority<Actor<R, S, A> | null> {
  return {
    async resolve(actor, scope, operation) {
      const principal = await access.enforce(actor, scope, ({ principal: verified, resource }) =>
        policy({ principal: verified, scope: resource, operation }),
      );
      return {
        kind: principal.kind,
        realmId: principal.realmId,
        subjectId: principal.subjectId,
      };
    },
  };
}
