import { defineEngineConfig } from "@lenso/engine/authoring";
import { runtimeEnginePlugin } from "@lenso/runtime/engine";
import { companionOptions } from "./src/companions";

export default defineEngineConfig({
  plugins: [
    runtimeEnginePlugin({
      ...companionOptions(),
      startCompanions: process.env.LENSO_RUNTIME_EXTERNAL !== "1",
    }),
  ],
});
