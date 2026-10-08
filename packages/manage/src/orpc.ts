import { os, ORPCError } from "@orpc/server";
import type { StandardSchemaV1 } from "@standard-schema/spec";
import type { EvidenceInput, FetchAuthContext } from "@lenso/auth/fetch";
import { diagnostic, environmentSecrets, redact } from "@lenso/engine/diagnostics";
import {
  boundedJson,
  validateOperations,
  type Operation,
  type OperationBoundOptions,
} from "@lenso/engine/operations";
import { createManageAdapter, type ManageAdapterOptions } from "./adapter";

export interface ManageRouterOptions<E, O extends Operation = Operation> extends Omit<
  ManageAdapterOptions<O>,
  "binding" | "canList"
> {
  readonly evidence: (context: FetchAuthContext) => EvidenceInput<E> | Promise<EvidenceInput<E>>;
  readonly binding: (
    operation: O,
    input: unknown,
    evidence: EvidenceInput<E>,
  ) => OperationBoundOptions<NoInfer<O>> | Promise<OperationBoundOptions<NoInfer<O>>>;
  readonly canList: (operation: O, evidence: EvidenceInput<E>) => boolean | Promise<boolean>;
}

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

export function createManageRouter<E, O extends Operation>(options: ManageRouterOptions<E, O>) {
  const { running, evidence: extractEvidence, binding, canList, maxOutputBytes } = options;
  const plugins = Object.freeze([...options.plugins]);
  const operations = Object.freeze([...options.operations]);
  validateOperations(plugins, operations);
  const base = os.$context<FetchAuthContext>();
  async function request<T>(
    context: FetchAuthContext,
    action: (adapter: ReturnType<typeof createManageAdapter>) => Promise<T>,
  ): Promise<T> {
    try {
      const evidence = await extractEvidence(context);
      const adapter = createManageAdapter({
        running,
        plugins,
        operations,
        ...(maxOutputBytes === undefined ? {} : { maxOutputBytes }),
        binding: (operation, input) => binding(operation, input, evidence),
        canList: (operation) => canList(operation, evidence),
      });
      return await action(adapter);
    } catch (error) {
      let detail: unknown = diagnostic(error, { instanceId: running.instanceId, phase: "invoke" });
      try {
        boundedJson(detail, 4096);
        detail = redact(detail, environmentSecrets());
        boundedJson(detail, 4096);
      } catch {
        detail = {
          code: "output-too-large",
          phase: "output",
          message: "Manage diagnostic exceeds the bounded error limit.",
        };
      }
      throw new ORPCError("MANAGE_FAILED", {
        message: "Manage request failed.",
        data: { schemaVersion: 1, diagnostic: detail },
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
