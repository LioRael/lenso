import { runtimeWorkspace } from "./src/workspace";

const runtimeRoot = process.env.LENSO_RUNTIME_ROOT;
const rustRoot = process.env.LENSO_RUST_ROOT;
if (!runtimeRoot || !rustRoot)
  throw new Error("Set LENSO_RUNTIME_ROOT and LENSO_RUST_ROOT to the trusted source checkouts.");
const implementation = process.env.LENSO_BATCH_IMPLEMENTATION ?? "rust";
if (implementation !== "rust" && implementation !== "ts")
  throw new Error("LENSO_BATCH_IMPLEMENTATION must be rust or ts.");

export default runtimeWorkspace({
  runtimeRoot,
  rustRoot,
  port: Number(process.env.LENSO_RUNTIME_PORT ?? 7331),
  implementation,
});
