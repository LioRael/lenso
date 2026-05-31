import { AlertTriangle, RotateCcw, X } from "lucide-react";

import { useRetryRuntimeWork } from "../../hooks/use-runtime-queries";
import { Button } from "../ui/button";
import { Dialog } from "../ui/dialog";
import { useRuntimeConsole } from "./runtime-console-context";
import { StatusPill } from "./status-pill";

export function RetryDialog() {
  const { closeRetry, retryTarget } = useRuntimeConsole();
  const retryMutation = useRetryRuntimeWork();

  return (
    <Dialog
      onOpenChange={(open) => !open && closeRetry()}
      open={Boolean(retryTarget)}
    >
      {retryTarget ? (
        <Dialog.Portal>
          <Dialog.Backdrop />
          <Dialog.Popup>
            <header className="flex items-center gap-3 border-b border-white/10 p-3">
              <div className="grid size-8 place-items-center border border-amber-300/30 bg-amber-300/10 text-amber-200">
                <AlertTriangle size={18} />
              </div>
              <div className="min-w-0">
                <p className="mb-1 font-mono text-[10px] font-semibold uppercase tracking-[0.08em] text-slate-600">
                  Retry confirmation
                </p>
                <Dialog.Title className="font-mono text-sm font-semibold text-slate-100">
                  Replay runtime work?
                </Dialog.Title>
              </div>
              <Button
                aria-label="Close retry dialog"
                className="ml-auto"
                onClick={closeRetry}
                variant="ghost"
              >
                <X size={15} />
              </Button>
            </header>

            <div className="grid gap-3 p-3">
              <div className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-3 border border-white/10 bg-white/[0.02] p-2.5">
                <StatusPill status={retryTarget.status} />
                <div className="min-w-0">
                  <div className="truncate font-mono text-[11px] font-semibold text-slate-100">
                    {retryTarget.name}
                  </div>
                  <div className="mono mt-0.5 truncate text-[10px] text-slate-600">
                    {retryTarget.kind} · {retryTarget.id}
                  </div>
                </div>
              </div>

              <dl className="grid grid-cols-[120px_minmax(0,1fr)] gap-x-3.5 gap-y-1.5 font-mono text-[11px] text-slate-400 [&_dd]:m-0 [&_dt]:text-slate-600">
                <dt>attempts</dt>
                <dd>
                  {retryTarget.attempts}/{retryTarget.maxAttempts}
                </dd>
                <dt>current status</dt>
                <dd>{retryTarget.status}</dd>
                <dt>operation</dt>
                <dd>reset to pending and make available now</dd>
              </dl>

              <Dialog.Description className="border border-amber-300/25 bg-amber-300/10 p-2.5 font-mono text-[11px] leading-5 text-amber-100">
                Retry is safe only when the handler is idempotent. Check
                downstream side effects before replaying this work.
              </Dialog.Description>
            </div>

            <footer className="flex justify-end gap-2 border-t border-white/10 p-3">
              <Button onClick={closeRetry} variant="ghost">
                Cancel
              </Button>
              <Button
                disabled={retryMutation.isPending}
                onClick={() => {
                  retryMutation.mutate(
                    { id: retryTarget.id, kind: retryTarget.kind },
                    { onSuccess: closeRetry }
                  );
                }}
                variant="danger"
              >
                <RotateCcw size={15} />
                {retryMutation.isPending ? "Retrying..." : "Retry"}
              </Button>
            </footer>
            {retryMutation.isError ? (
              <div className="mono mx-3 mb-3 border border-rose-300/30 bg-black/20 p-2.5 text-[11px] text-rose-100">
                {errorMessage(retryMutation.error)}
              </div>
            ) : null}
          </Dialog.Popup>
        </Dialog.Portal>
      ) : null}
    </Dialog>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Retry failed";
}
