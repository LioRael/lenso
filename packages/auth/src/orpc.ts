import { ORPCError, os } from "@orpc/server";
import { AuthError, authErrorDiagnostic } from "./errors";
import type { AuthenticationOptions } from "./core";
import type { EvidenceInput } from "./fetch";

interface AuthenticationAccess<E, P> {
  optional(evidence: E, options?: AuthenticationOptions): Promise<P | null>;
  required(evidence: E, options?: AuthenticationOptions): Promise<P>;
}

type EvidenceExtractor<C, E> = ((context: C) => EvidenceInput<E>) &
  ("actor" extends keyof C ? never : unknown);

function transportError(error: unknown): never {
  const safe = authErrorDiagnostic(error);
  if (safe) {
    const code = safe.code === "REAUTHENTICATION_REQUIRED" ? "UNAUTHORIZED" : safe.code;
    throw new ORPCError(code, { message: safe.message, cause: error });
  }
  throw new ORPCError("SERVICE_UNAVAILABLE", {
    message: "Authentication unavailable",
    cause: error,
  });
}

function actorContext<C, P>(actor: P) {
  // EvidenceExtractor forbids an existing actor; satisfy the generic overlap constraint.
  return { actor } as { actor: P } & Pick<C, keyof C & "actor">;
}

export function optionalAuth<C extends object, E, P>(
  access: AuthenticationAccess<E, P>,
  evidence: EvidenceExtractor<C, E>,
) {
  return os.$context<C>().middleware(async ({ context, next }) => {
    let input: EvidenceInput<E> | undefined;
    let actor: P | null;
    try {
      input = evidence(context);
      actor = await access.optional(input.evidence, { signal: input.signal });
    } catch (error) {
      input?.signal?.throwIfAborted();
      transportError(error);
    }
    try {
      return await next({ context: actorContext<C, P | null>(actor) });
    } catch (error) {
      input!.signal?.throwIfAborted();
      if (error instanceof AuthError) transportError(error);
      throw error;
    }
  });
}

export function requiredAuth<C extends object, E, P>(
  access: AuthenticationAccess<E, P>,
  evidence: EvidenceExtractor<C, E>,
) {
  return os.$context<C>().middleware(async ({ context, next }) => {
    let input: EvidenceInput<E> | undefined;
    let actor: P;
    try {
      input = evidence(context);
      actor = await access.required(input.evidence, { signal: input.signal });
    } catch (error) {
      input?.signal?.throwIfAborted();
      transportError(error);
    }
    try {
      return await next({ context: actorContext<C, P>(actor) });
    } catch (error) {
      input!.signal?.throwIfAborted();
      if (error instanceof AuthError) transportError(error);
      throw error;
    }
  });
}
