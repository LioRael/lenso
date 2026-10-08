import { join, resolve } from "node:path";
import { startApp } from "@lenso/core";
import { readApplication } from "@lenso/engine/application";
import { EngineError } from "@lenso/engine/diagnostics";
import { describePluginConfig } from "@lenso/engine";
import {
  invokeValidatedOperation,
  resolveOperation,
  validateOperationInput,
  type OperationBinding,
  type Operation,
} from "@lenso/engine/operations";
import { CliError, diagnostic, environmentSecrets, exitCode, redact } from "./diagnostics";
import { describeOperation, redactOperationDescription } from "./operations";
import type { AppDefinition } from "@lenso/engine/application";
export type { AppDefinition } from "@lenso/engine/application";

export async function invoke<O extends Operation>(
  app: AppDefinition<O>,
  pluginId: string,
  method: string,
  input: unknown,
  binding: OperationBinding<NoInfer<O>> | undefined = app.operationBinding,
): Promise<unknown> {
  const plugin = app.plugins.find((candidate) => candidate.id === pluginId);
  if (!plugin)
    throw new CliError(
      { code: "unknown-plugin", phase: "discovery", message: "Unknown plugin.", pluginId },
      3,
    );
  let operation: O;
  let validatedInput: unknown;
  try {
    operation = resolveOperation(app.plugins, app.operations ?? [], pluginId, method) as O;
    validatedInput = await validateOperationInput(operation, input);
  } catch (cause) {
    throw new CliError(
      diagnostic(cause, { pluginId, operation: `${pluginId}.${method}` }),
      exitCode(cause),
      { cause },
    );
  }
  const context = {
    pluginId,
    operation: `${pluginId}.${method}`,
    ...(operation?.source ? { source: operation.source } : {}),
  };
  let running;
  try {
    running = await startApp(app);
  } catch (cause) {
    throw new CliError(diagnostic(cause, { ...context, phase: "setup" }), 1);
  }
  let result: unknown;
  let callFailed = false;
  let callError: unknown;
  try {
    const options = binding ? await binding(operation, validatedInput, running) : undefined;
    result = await invokeValidatedOperation<Operation>(running, operation, validatedInput, options);
  } catch (cause) {
    callFailed = true;
    callError =
      cause instanceof CliError
        ? cause
        : cause instanceof EngineError
          ? new CliError(
              diagnostic(cause, { ...context, phase: "invoke" }),
              exitCode(cause.cause instanceof CliError ? cause.cause : cause),
              {
                cause,
              },
            )
          : new CliError(
              {
                code: "invocation-failed",
                phase: "invoke",
                message: "Service invocation failed.",
                ...context,
              },
              1,
              { cause },
            );
  }
  try {
    await running.stop();
  } catch (cleanupError) {
    if (callFailed)
      throw new CliError({
        code: "invocation-and-cleanup-failed",
        phase: "invoke",
        message: "Service invocation and cleanup failed.",
        ...context,
        causes: [
          diagnostic(callError, context),
          diagnostic(cleanupError, { ...context, phase: "cleanup" }),
        ],
      });
    throw new CliError(diagnostic(cleanupError, { ...context, phase: "cleanup" }));
  }
  if (callFailed) throw callError;
  return result;
}

export async function call(
  root: string,
  pluginId: string,
  method: string,
  input: unknown,
): Promise<unknown> {
  const directory = resolve(root);
  const { app } = await readApplication(directory, join(directory, "lenso.config.ts"));
  try {
    return await invoke(app, pluginId, method, input);
  } catch (cause) {
    throw new CliError(
      diagnostic(cause, { source: { file: join(resolve(root), "lenso.config.ts") } }),
      exitCode(cause),
      { cause },
    );
  }
}

/** Imports trusted config but never runs setup or discovers methods by reflection. */
export async function inspect(root = process.cwd(), pluginId?: string, method?: string) {
  const directory = resolve(root);
  const { app, ordered, configPath } = await readApplication(
    directory,
    join(directory, "lenso.config.ts"),
  );
  if (pluginId && !ordered.some((plugin) => plugin.id === pluginId))
    throw new CliError(
      {
        code: "unknown-plugin",
        phase: "discovery",
        message: "Unknown plugin.",
        pluginId,
        source: { file: configPath },
      },
      3,
    );
  const operations = (app.operations ?? []).filter(
    (operation) =>
      (!pluginId || operation.plugin.id === pluginId) && (!method || operation.method === method),
  );
  if (method && !operations.length)
    throw new CliError(
      {
        code: "unknown-operation",
        phase: "discovery",
        message: "Operation is not declared.",
        ...(pluginId ? { pluginId } : {}),
        ...(method ? { operation: `${pluginId ?? "*"}.${method}` } : {}),
        source: { file: configPath },
      },
      3,
    );
  return {
    configPath,
    inspection: "static" as const,
    plugins: ordered.map((plugin) => ({
      id: plugin.id,
      requires: (plugin.requires ?? []).map((dependency) => dependency.id),
      source: plugin.source ?? { file: configPath },
      contributions: redact(plugin.contributions ?? [], environmentSecrets()),
      ...(plugin.config ? { config: describePluginConfig(plugin, configPath) } : {}),
    })),
    operations: operations.map((operation) =>
      redactOperationDescription(describeOperation(operation, configPath)),
    ),
    limitations: [
      "Imports trusted config and executes module top-level code.",
      "Executes trusted schema converters while describing operations and configuration; inspection is not a sandbox.",
      "Never runs plugin setup; runtime-only methods and authorization outcomes cannot be discovered.",
      "Never invokes configuration source reads or exposes configuration values, revisions or runtime provenance.",
      "Does not load Engine config or infer service methods, provider descriptors, or health from runtime state.",
      "Each call starts and stops an isolated app; no automatic retry, cancellation or request drain.",
    ],
  };
}
