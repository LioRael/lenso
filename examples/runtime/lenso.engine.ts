import { defineEngineConfig } from "@lenso/engine/authoring";
import { runtimeContractPlugin, runtimeEnginePlugin } from "@lenso/runtime/engine";
import { companionOptions } from "./src/companions";

export default defineEngineConfig({
  plugins: [
    runtimeContractPlugin({ source: "src/runtime-contract.ts", bindings: { name: "logs" } }),
    // Generation needs no prebuilt binaries or companion processes.
    ...(process.env.LENSO_RUNTIME_ROOT
      ? [
          runtimeEnginePlugin({
            ...companionOptions(),
            startCompanions: process.env.LENSO_RUNTIME_EXTERNAL !== "1",
          }),
        ]
      : []),
  ],
});
