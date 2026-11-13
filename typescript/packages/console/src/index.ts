import { z } from "zod";
import { ActionError, validateConsoleManifest } from "@lenso/app";
import type { ConsoleManifest, ConsolePage, Parser } from "@lenso/app";

export interface ConsoleViewContext {
  readonly client: ConsoleClient;
  readonly page: ConsolePage;
}

export interface ConsolePageExtension<View> {
  readonly id: string;
  readonly pages: Readonly<Record<string, (context: ConsoleViewContext) => View>>;
}

/** Registers compiled custom pages in a Console host; remote metadata never supplies executable code. */
export class ConsolePages<View> {
  private readonly extensions = new Set<string>();
  private readonly pages = new Map<string, (context: ConsoleViewContext) => View>();

  constructor(private readonly client: ConsoleClient) {}

  register(extension: ConsolePageExtension<View>): void {
    if (this.extensions.has(extension.id) || Object.keys(extension.pages).some((id) => this.pages.has(id))) {
      throw new ActionError("conflict", "Console extension or page already installed");
    }
    for (const [id, view] of Object.entries(extension.pages)) this.pages.set(id, view);
    this.extensions.add(extension.id);
  }

  render(page: ConsolePage): View {
    const view = this.pages.get(page.id);
    if (!view) throw new ActionError("not_found", `Console page extension not installed: ${page.id}`);
    return view({ client: this.client, page });
  }
}

const manifestSchema = z.object({
  version: z.literal(1),
  modules: z.array(z.object({
    id: z.string(), actions: z.array(z.string()), tasks: z.array(z.string()),
    events: z.array(z.string()), resources: z.array(z.string()),
  })),
  pages: z.array(z.object({
    id: z.string(), title: z.string(), list: z.string(),
    detail: z.string().nullish().transform((value) => value ?? undefined),
    actions: z.array(z.string()).optional(),
  })),
});
const errorSchema = z.object({ error: z.object({ code: z.enum([
  "invalid_input", "unauthenticated", "forbidden", "not_found", "conflict", "cancelled", "internal",
]), message: z.string() }) });

/** A UI-independent client. Management authorization is always enforced by the application. */
export class ConsoleClient {
  private readonly base: URL;

  constructor(
    baseUrl: string,
    private readonly token: () => string | Promise<string>,
    private readonly fetcher: typeof fetch = globalThis.fetch,
  ) {
    this.base = new URL(baseUrl);
    if (!this.base.pathname.endsWith("/")) this.base.pathname += "/";
  }

  async manifest(signal?: AbortSignal): Promise<ConsoleManifest> {
    const value = await this.request("manifest", "GET", undefined, signal);
    try {
      const manifest = manifestSchema.parse(value);
      const description: ConsoleManifest = {
        ...manifest,
        pages: manifest.pages.map((page) => ({
          id: page.id, title: page.title, list: page.list,
          ...(page.detail === undefined ? {} : { detail: page.detail }),
          ...(page.actions === undefined ? {} : { actions: page.actions }),
        })),
      };
      validateConsoleManifest(description);
      return description;
    } catch (cause) { throw new ActionError("internal", "Invalid Console manifest", { cause }); }
  }

  /** Output remains unknown until the extension validates its own action's result. */
  call(action: string, input: unknown = null, signal?: AbortSignal): Promise<unknown> {
    return this.request(`actions/${encodeURIComponent(action)}`, "POST", input, signal);
  }

  /** Parses only the response; server failures retain their code and parser failures are internal. */
  async callParsed<T>(action: string, result: Parser<T>, input: unknown = null, signal?: AbortSignal): Promise<T> {
    const value = await this.call(action, input, signal);
    try { return await result.parse(value); }
    catch (cause) { throw new ActionError("internal", "Invalid Console action response", { cause }); }
  }

  private async request(path: string, method: string, body: unknown, signal?: AbortSignal): Promise<unknown> {
    signal?.throwIfAborted();
    const token = await this.token();
    signal?.throwIfAborted();
    const url = new URL(path, this.base);
    url.search = this.base.search;
    const response = await this.fetcher.call(globalThis, url, {
      method, headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
      ...(signal === undefined ? {} : { signal }),
      redirect: "error",
    });
    signal?.throwIfAborted();
    let value: unknown;
    try { value = await response.json(); }
    catch (cause) { signal?.throwIfAborted(); throw new ActionError("internal", "Invalid Console response", { cause }); }
    signal?.throwIfAborted();
    if (!response.ok) {
      const parsed = errorSchema.safeParse(value);
      if (!parsed.success) throw new ActionError("internal", `Management request failed (${response.status})`);
      throw new ActionError(parsed.data.error.code, parsed.data.error.message);
    }
    return value;
  }
}
