import { os, ORPCError } from "@orpc/server";
import type { StandardSchemaV1 } from "@standard-schema/spec";
import type { EvidenceInput, FetchAuthContext } from "@lenso/auth/fetch";
import { AuthError } from "@lenso/auth";
import { EngineError } from "@lenso/engine/diagnostics";
import { type Operation, type OperationBoundOptions } from "@lenso/engine/operations";
import {
  createManageAdapter,
  createManageSelection,
  type ManageRequestOptions,
  type ManageSelection,
  type ManageSelectionOptions,
} from "./adapter";

export type ManageRouterOptions<E, O extends Operation = Operation> = Omit<
  ManageRequestOptions<O>,
  "binding" | "canList"
> &
  (ManageSelectionOptions<O> | { readonly selection: ManageSelection<O> }) & {
    readonly evidence: (context: FetchAuthContext) => EvidenceInput<E> | Promise<EvidenceInput<E>>;
    readonly binding: (
      operation: O,
      input: unknown,
      evidence: EvidenceInput<E>,
    ) => OperationBoundOptions<NoInfer<O>> | Promise<OperationBoundOptions<NoInfer<O>>>;
    readonly canList: (operation: O, evidence: EvidenceInput<E>) => boolean | Promise<boolean>;
  };

type InvocationEnvelope =
  | {
      pluginId: string;
      method: string;
      input: unknown;
    }
  | { key: string; input: unknown };

const envelope: StandardSchemaV1<unknown, InvocationEnvelope> = {
  "~standard": {
    version: 1,
    vendor: "lenso-manage",
    validate(value) {
      if (
        value &&
        typeof value === "object" &&
        !Array.isArray(value) &&
        typeof Reflect.get(value, "key") === "string" &&
        Reflect.get(value, "key") &&
        Object.hasOwn(value, "input") &&
        Object.keys(value).every((key) => key === "key" || key === "input")
      )
        return { value: { key: Reflect.get(value, "key"), input: Reflect.get(value, "input") } };
      if (
        !value ||
        typeof value !== "object" ||
        Array.isArray(value) ||
        typeof Reflect.get(value, "pluginId") !== "string" ||
        !Reflect.get(value, "pluginId") ||
        typeof Reflect.get(value, "method") !== "string" ||
        !Reflect.get(value, "method") ||
        !Object.hasOwn(value, "input") ||
        Object.keys(value).some((key) => !["pluginId", "method", "input"].includes(key))
      )
        return { issues: [{ message: "Expected pluginId, method and raw input." }] };
      return {
        value: {
          pluginId: Reflect.get(value, "pluginId"),
          method: Reflect.get(value, "method"),
          input: Reflect.get(value, "input"),
        },
      };
    },
  },
};

const classifications = {
  BAD_REQUEST: { message: "Manage input is invalid." },
  UNAUTHORIZED: { message: "Authentication is required." },
  FORBIDDEN: { message: "Permission denied." },
  NOT_FOUND: { message: "Manage operation or resource not found." },
  CONFLICT: { message: "Manage request conflicts with current state." },
  PAYLOAD_TOO_LARGE: { message: "Manage input exceeds the allowed size." },
  SERVICE_UNAVAILABLE: { message: "Manage service is unavailable." },
  BAD_GATEWAY: { message: "Manage provider operation failed." },
  NOT_IMPLEMENTED: { message: "Manage operation is not supported." },
  CLIENT_CLOSED_REQUEST: { message: "Request cancelled; operation effects may have completed." },
  MANAGE_FAILED: { message: "Manage request failed." },
} as const;
type PublicCode = keyof typeof classifications;
type SafeErrorData = {
  schemaVersion: 1;
  diagnostic: { code: PublicCode; phase: "invoke"; message: string };
};
const safeErrorData: StandardSchemaV1<unknown, SafeErrorData> = {
  "~standard": {
    version: 1,
    vendor: "lenso-manage",
    validate(value) {
      if (value && typeof value === "object" && Reflect.get(value, "schemaVersion") === 1) {
        const detail = Reflect.get(value, "diagnostic");
        const code = detail && typeof detail === "object" ? Reflect.get(detail, "code") : undefined;
        if (typeof code === "string" && Object.hasOwn(classifications, code)) {
          const safeCode = code as PublicCode;
          if (detail.phase === "invoke" && detail.message === classifications[safeCode].message)
            return { value: publicData(safeCode) };
        }
      }
      return { issues: [{ message: "Invalid safe Manage error data." }] };
    },
  },
};
function publicData(code: PublicCode): SafeErrorData {
  return {
    schemaVersion: 1,
    diagnostic: { code, phase: "invoke", message: classifications[code].message },
  };
}
function publicCode(error: unknown): PublicCode {
  let code: string | undefined;
  if (error instanceof AuthError) code = error.code;
  else if (error instanceof EngineError) {
    code = error.diagnostic.code;
    if (["runtime-failed", "invocation-failed"].includes(code) && error.cause instanceof AuthError)
      code = error.cause.code;
  }
  switch (code) {
    case "invalid-input":
    case "invalid-key":
    case "invalid-task":
    case "invalid-options":
      return "BAD_REQUEST";
    case "UNAUTHORIZED":
    case "REAUTHENTICATION_REQUIRED":
      return "UNAUTHORIZED";
    case "FORBIDDEN":
    case "forbidden":
    case "permission-denied":
    case "confirmation-required":
    case "approval-required":
      return "FORBIDDEN";
    case "unknown-plugin":
    case "unknown-operation":
    case "forbidden-operation":
    case "not-found":
    case "job-expired":
      return "NOT_FOUND";
    case "conflict":
    case "deduplication-conflict":
      return "CONFLICT";
    case "too-large":
      return "PAYLOAD_TOO_LARGE";
    case "SERVICE_UNAVAILABLE":
    case "provider-unavailable":
    case "closed":
      return "SERVICE_UNAVAILABLE";
    case "provider":
      return "BAD_GATEWAY";
    case "unsupported":
      return "NOT_IMPLEMENTED";
    case "aborted":
      return "CLIENT_CLOSED_REQUEST";
    default:
      return "MANAGE_FAILED";
  }
}

export function createManageRouter<E, O extends Operation>(options: ManageRouterOptions<E, O>) {
  const selection = "selection" in options ? options.selection : createManageSelection(options);
  return routerForSelection(selection, options);
}

function routerForSelection<E, O extends Operation>(
  selection: ManageSelection<O>,
  {
    evidence: extractEvidence,
    binding,
    canList,
    maxOutputBytes,
  }: Pick<ManageRouterOptions<E, O>, "evidence" | "binding" | "canList" | "maxOutputBytes">,
) {
  const base = os.$context<FetchAuthContext>().errors({
    BAD_REQUEST: { ...classifications.BAD_REQUEST, data: safeErrorData },
    UNAUTHORIZED: { ...classifications.UNAUTHORIZED, data: safeErrorData },
    FORBIDDEN: { ...classifications.FORBIDDEN, data: safeErrorData },
    NOT_FOUND: { ...classifications.NOT_FOUND, data: safeErrorData },
    CONFLICT: { ...classifications.CONFLICT, data: safeErrorData },
    PAYLOAD_TOO_LARGE: { ...classifications.PAYLOAD_TOO_LARGE, data: safeErrorData },
    SERVICE_UNAVAILABLE: { ...classifications.SERVICE_UNAVAILABLE, data: safeErrorData },
    BAD_GATEWAY: { ...classifications.BAD_GATEWAY, data: safeErrorData },
    NOT_IMPLEMENTED: { ...classifications.NOT_IMPLEMENTED, data: safeErrorData },
    CLIENT_CLOSED_REQUEST: { ...classifications.CLIENT_CLOSED_REQUEST, data: safeErrorData },
    MANAGE_FAILED: { ...classifications.MANAGE_FAILED, data: safeErrorData },
  });
  async function request<T>(
    context: FetchAuthContext,
    action: (adapter: ReturnType<typeof createManageAdapter>) => Promise<T>,
  ): Promise<T> {
    try {
      const evidence = await extractEvidence(context);
      const adapter = createManageAdapter({
        selection,
        ...(maxOutputBytes === undefined ? {} : { maxOutputBytes }),
        binding: (operation, input) => binding(operation, input, evidence),
        canList: (operation) => canList(operation, evidence),
      });
      return await action(adapter);
    } catch (error) {
      const code = publicCode(error);
      throw new ORPCError(code, {
        ...classifications[code],
        data: publicData(code),
        cause: error,
      });
    }
  }
  return {
    catalog: base.handler(({ context }) => request(context, (adapter) => adapter.catalog())),
    invoke: base
      .input(envelope)
      .handler(({ context, input }) =>
        request(context, (adapter) =>
          "key" in input
            ? adapter.invokeEntry(input.key, input.input)
            : adapter.invoke(input.pluginId, input.method, input.input),
        ),
      ),
  };
}
