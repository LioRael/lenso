import { serveBorrowedStdio } from "@lenso/mcp";
import { startHost, testOnlyIdentity } from "./host";

// Dedicated local process, fixed test-only launch identity. A real launcher
// supplies its trusted local identity; tool JSON must never supply it.
const host = await startHost();
let adapter: Awaited<ReturnType<typeof serveBorrowedStdio>> | undefined;
let onEnd: (() => void) | undefined;
try {
  adapter = await serveBorrowedStdio({ ...host.options, identity: testOnlyIdentity });
  await new Promise<void>((resolve) => {
    onEnd = resolve;
    process.stdin.once("end", onEnd);
    process.once("SIGINT", onEnd);
    process.once("SIGTERM", onEnd);
    if (process.stdin.readableEnded) resolve();
  });
} finally {
  if (onEnd) {
    process.stdin.off("end", onEnd);
    process.off("SIGINT", onEnd);
    process.off("SIGTERM", onEnd);
  }
  try {
    await adapter?.close();
  } finally {
    await host.app.stop();
  }
}
