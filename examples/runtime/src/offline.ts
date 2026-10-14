import { startApp } from "@lenso/core";
import app, { batch } from "./batch-app";

const running = await startApp(app);
try {
  console.log(
    JSON.stringify(
      running.get(batch).aggregate({
        logs: [
          { level: "info", message: "synthetic request", duration_ms: 12 },
          { level: "error", message: "synthetic failure", duration_ms: 35 },
        ],
      }),
    ),
  );
} finally {
  await running.stop();
}
