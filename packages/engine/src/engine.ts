import { resolve } from "node:path";
import { readApplication, type Discovery, type PluginManifest } from "./application";
import { EngineSession, withEngine } from "./engine-host";
import { defaultEnginePlugins, pluginManifest } from "./engine-defaults";
import type { EngineMode, EngineSnapshot } from "./engine-authoring";

export interface PreparedEngine {
  readonly session: EngineSession;
  prepare(): Promise<Discovery>;
}
export function createEngineSession(root: string, mode: EngineMode): PreparedEngine {
  const session = new EngineSession(resolve(root), mode);
  let app: Promise<Discovery> | undefined;
  let prepared: Promise<Discovery> | undefined;
  const readApp = (snapshot: EngineSnapshot) =>
    (app ??= readApplication(session.root, resolve(session.root, snapshot.convention.config)));
  return {
    session,
    prepare() {
      return (prepared ??= (async () => {
        await session.setup(defaultEnginePlugins(readApp));
        const snapshot = await session.discover();
        return readApp(snapshot);
      })());
    },
  };
}
/** Build-time discovery validates trusted extensions, then closes their resources. */
export async function discover(root = process.cwd()): Promise<Discovery> {
  const engine = createEngineSession(root, "check");
  return withEngine(engine.session, engine.prepare);
}
/** Static generation starts engine plugins, never runtime application plugin setup. */
export async function generate(root = process.cwd()): Promise<readonly PluginManifest[]> {
  const engine = createEngineSession(root, "generate");
  return withEngine(engine.session, async () => {
    const app = await engine.prepare();
    await engine.session.generate();
    return pluginManifest(app);
  });
}
export async function build(root = process.cwd(), entry?: string): Promise<string> {
  const engine = createEngineSession(root, "build");
  return withEngine(engine.session, async () => {
    await engine.prepare();
    await engine.session.generate();
    return engine.session.build(entry);
  });
}
