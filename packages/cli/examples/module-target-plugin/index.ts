import { fileURLToPath } from "node:url";
import { defineEnginePlugin, type BundleOptions } from "@lenso/engine/authoring";

/** Reuse the Engine bundler for a browser/Workers module; no CLI source changes. */
export function moduleTarget(options: {
  name: string;
  entry: string;
  platform?: BundleOptions["platform"];
}) {
  return defineEnginePlugin({
    name: "example/module-target",
    source: { file: fileURLToPath(import.meta.url), export: "moduleTarget" },
    setup(context) {
      context.watch(options.entry);
      context.target(options.name, (build) =>
        build.bundle({
          entry: options.entry,
          platform: options.platform ?? "browser",
          packages: "bundle",
          directory: options.name,
        }),
      );
    },
  });
}
