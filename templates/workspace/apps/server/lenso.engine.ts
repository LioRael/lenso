import { defineEngineConfig, defineEnginePlugin } from "@lenso/engine/authoring";

export default defineEngineConfig({
  plugins: [
    defineEnginePlugin({
      name: "app/server-entry",
      setup(context) {
        context.convention(() => ({ config: "lenso.config.ts", entry: "runtime/main.ts" }), {
          replace: "lenso/defaults",
        });
      },
    }),
  ],
});
