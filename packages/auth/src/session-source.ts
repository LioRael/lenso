import type {
  AuthSource,
  AuthenticationResult,
  SessionEvidence,
  SourceCapabilities,
  VerificationContext,
} from "./source";
import { AuthError } from "./errors";

/** External session ownership remains with the supplied getSession implementation. */
export function sessionSource<Session, S extends string = string>(options: {
  getSession(input: { headers: Headers }, context: VerificationContext): Promise<Session | null>;
  subjectId(session: Session): S;
  hasCredential?(headers: Headers): boolean;
  session?(session: Session): SessionEvidence;
  readonly capabilities?: SourceCapabilities;
}): AuthSource<Headers, S> {
  return {
    capabilities: options.capabilities,
    async verify(headers, context): Promise<AuthenticationResult<S>> {
      const { signal } = context;
      signal.throwIfAborted();
      if (options.hasCredential) {
        const present = options.hasCredential(headers);
        if (present === false) return { status: "absent" };
        if (present !== true) throw new AuthError("SERVICE_UNAVAILABLE");
      }
      const session = await options.getSession({ headers }, context);
      signal.throwIfAborted();
      if (session === null) {
        return { status: options.hasCredential ? "rejected" : "unresolved" };
      }
      return {
        status: "verified",
        subjectId: options.subjectId(session),
        ...(options.session ? { session: options.session(session) } : {}),
      };
    },
  };
}
