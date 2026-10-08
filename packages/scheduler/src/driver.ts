import type { Scheduler } from "./index";
import { SchedulerError } from "./errors";

/** Explicit native host driver. Platform scheduled events should await tick() instead. */
export function startSchedulerDriver(
  scheduler: Pick<Scheduler, "tick">,
  options: { readonly intervalMs?: number } = {},
) {
  const intervalMs = options.intervalMs ?? 1000;
  if (!Number.isSafeInteger(intervalMs) || intervalMs < 1 || intervalMs > 2_147_483_647)
    throw new SchedulerError("invalid-options");
  let stopping = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let wake: (() => void) | undefined;
  const done = (async () => {
    while (!stopping) {
      await scheduler.tick();
      if (stopping) break;
      await new Promise<void>((resolve) => {
        wake = resolve;
        timer = setTimeout(resolve, intervalMs);
      });
    }
  })();
  return {
    done,
    async stop() {
      stopping = true;
      if (timer !== undefined) clearTimeout(timer);
      wake?.();
      await done;
    },
  };
}
