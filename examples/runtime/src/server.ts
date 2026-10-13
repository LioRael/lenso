import { startRuntimeApp } from "@lenso/runtime";
import { reportDevReady } from "@lenso/engine/dev-ready";
import app from "../lenso.config";
import { logs } from "./plugins";

const running = await startRuntimeApp(app);
let closing: Promise<void> | undefined;
const close = () => (closing ??= running.stop());
for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.once(signal, () => {
    void close().then(
      () => process.exit(0),
      (error) => {
        console.error(error);
        process.exit(1);
      },
    );
  });
}
try {
  const service = running.get(logs);
  const input = {
    logs: [
      { level: "info", message: "synthetic request", duration_ms: 12 },
      { level: "error", message: "synthetic failure", duration_ms: 35 },
    ],
  };
  console.log(JSON.stringify(await service.aggregate(input)));
  for await (const chunk of service.progress({ ...input, delay_ms: 10 })) {
    console.log(JSON.stringify({ progress: chunk }));
  }
  reportDevReady({ capabilities: ["runtime/logs"] });
  await new Promise<void>((resolve) => {
    const keepAlive = setInterval(() => {}, 60000);
    process.once("SIGTERM", () => {
      clearInterval(keepAlive);
      resolve();
    });
    process.once("SIGINT", () => {
      clearInterval(keepAlive);
      resolve();
    });
  });
} finally {
  await close();
}
