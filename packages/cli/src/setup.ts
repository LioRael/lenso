import { pathToFileURL } from "node:url";
import {
  isInitialization,
  type InitializationAction,
  type InitializationStatus,
} from "@lenso/core/initialization";
import {
  applicationConfigPath,
  readApplication,
  resolveApplicationTarget,
  type ApplicationTarget,
} from "@lenso/engine/application";
import { CliError } from "./diagnostics";

/** Separate trusted bootstrap entry: never starts the incomplete application or Engine. */
export async function setup(
  target: string | ApplicationTarget,
  action: InitializationAction,
  input?: unknown,
): Promise<InitializationStatus> {
  const selected = resolveApplicationTarget(target);
  const configPath = applicationConfigPath(selected);
  const { app } = await readApplication(selected.root, configPath);
  const loaded = await import(pathToFileURL(configPath).href);
  if (!isInitialization(loaded.initialization) || app.initialization !== loaded.initialization)
    throw new CliError(
      {
        code: "initialization-unavailable",
        phase: "discovery",
        message:
          "Select a config with the same explicit initialization definition on defineApp and its named export.",
      },
      3,
    );
  if (typeof loaded.initializationBinding !== "function")
    throw new CliError({
      code: "initialization-unauthorized",
      phase: "config",
      message:
        "The application must supply a trusted initializationBinding; JSON does not grant bootstrap authority.",
    });
  let evidence: unknown;
  try {
    evidence = await loaded.initializationBinding(action);
  } catch {
    throw new CliError({
      code: "initialization-unauthorized",
      phase: "config",
      message: "The trusted initialization entry could not authorize this action.",
    });
  }
  if (action === "status") return loaded.initialization.status(evidence);
  if (action === "activate") return loaded.initialization.activate(evidence);
  if (
    !input ||
    typeof input !== "object" ||
    Array.isArray(input) ||
    Object.keys(input).some((key) => !["requestId", "input"].includes(key))
  )
    throw new CliError(
      {
        code: "initialization-invalid-input",
        phase: "input",
        message: "setup apply expects {requestId,input} on stdin or in an input file.",
      },
      2,
    );
  return loaded.initialization.apply(input as { requestId: string; input: unknown }, evidence);
}
