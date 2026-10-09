import { basename, resolve } from "node:path";
import pc from "picocolors";
import sdkPackage from "@lenso/core/package.json";

export interface DevReady {
  /** Actual listener URLs, reported only after application startup succeeds. */
  readonly urls?: readonly (string | URL)[];
  /** Enabled capability names, supplied by the running application. */
  readonly capabilities?: readonly string[];
}

export interface DevPresentationOptions {
  readonly project: string;
  /** JSON/protocol callers own all output; presentation is completely silent. */
  readonly mode?: "human" | "json";
  readonly output?: { readonly isTTY?: boolean; write(text: string): unknown };
  readonly environment?: Partial<Pick<NodeJS.ProcessEnv, "NO_COLOR" | "CI" | "TERM">>;
  readonly now?: () => number;
}

export interface DevPresentation {
  starting(): void;
  ready(info?: DevReady): void;
  /** Emit a safe status only. Error details belong to the CLI diagnostic layer. */
  failed(): void;
}

function singleLine(value: string): string {
  // Prevent project/capability text from inserting terminal controls or extra rows.
  return value.replace(/\p{Cc}/gu, " ").trim();
}

function listenerUrl(value: string | URL): string | undefined {
  try {
    const url = new URL(String(value));
    if (url.protocol !== "http:" && url.protocol !== "https:") return undefined;
    // Wildcard binds are not navigable addresses; never guess a network interface.
    if (["0.0.0.0", "[::]"].includes(url.hostname)) return undefined;
    // Display the listener origin only; credentials, query and path can contain secrets.
    return `${url.origin}/`;
  } catch {
    return undefined;
  }
}

/** A small stderr-only human view. It never discovers resources or infers readiness. */
export function createDevPresentation(options: DevPresentationOptions): DevPresentation {
  const output = options.output ?? process.stderr;
  const environment = options.environment ?? process.env;
  const color = pc.createColors(
    Boolean(output.isTTY) &&
      environment.NO_COLOR === undefined &&
      !environment.CI &&
      environment.TERM !== "dumb",
  );
  const now = options.now ?? (() => performance.now());
  const project = resolve(options.project);
  let state: "idle" | "starting" | "ready" | "failed" = "idle";
  let startedAt = 0;
  let displayedHeader = false;

  function write(text: string): void {
    if (options.mode !== "json") output.write(text);
  }

  return {
    starting() {
      state = "starting";
      startedAt = now();
      if (!displayedHeader) {
        displayedHeader = true;
        write(
          `\n  ${color.bold(color.cyan("Lenso"))} ${sdkPackage.version} ${color.dim(`· Bun ${Bun.version}`)}\n` +
            `  ${color.dim("Project:")} ${singleLine(basename(project))}\n` +
            `  ${color.dim("Root:")}    ${singleLine(project)}\n\n`,
        );
      }
      write(`  ${color.yellow("Starting...")}\n`);
    },
    ready(info = {}) {
      // A caller cannot accidentally emit Ready before startup, twice, or after failure.
      if (state !== "starting") return;
      state = "ready";
      const elapsed = Math.max(0, Math.round(now() - startedAt));
      const urls = [
        ...new Set((info.urls ?? []).map(listenerUrl).filter((url): url is string => !!url)),
      ];
      const capabilities = [...new Set((info.capabilities ?? []).map(singleLine).filter(Boolean))];
      for (const url of urls) write(`  ${color.dim("URL:")}     ${color.cyan(url)}\n`);
      if (capabilities.length) {
        write(`  ${color.dim("Enabled:")} ${capabilities.join(", ")}\n`);
      }
      write(`  ${color.green("Ready")} ${color.dim(`in ${elapsed} ms`)}\n\n`);
    },
    failed() {
      state = "failed";
      write(`  ${color.red("Failed.")} See diagnostics for details; watching for changes.\n\n`);
    },
  };
}
