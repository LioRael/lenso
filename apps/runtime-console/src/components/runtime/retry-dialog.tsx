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
            <header className="flex items-center gap-3 border-b border-white/10 p-4">
              <div className="grid size-9 place-items-center rounded-lg border border-amber-300/30 bg-amber-300/10 text-amber-200">
                <AlertTriangle size={18} />
              </div>
              <div className="min-w-0">
                <p className="mb-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-slate-500">
                  Retry confirmation
                </p>
                <Dialog.Title className="text-lg font-semibold text-slate-100">
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

            <div className="grid gap-3.5 p-4">
              <div className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-3 rounded-lg border border-white/10 bg-white/[0.025] p-3">
                <StatusPill status={retryTarget.status} />
                <div className="min-w-0">
                  <div className="truncate text-[13px] font-semibold text-slate-100">
                    {retryTarget.name}
                  </div>
                  <div className="mono mt-0.5 truncate text-xs text-slate-500">
                    {retryTarget.kind} · {retryTarget.id}
                  </div>
                </div>
              </div>

              <dl className="grid grid-cols-[120px_minmax(0,1fr)] gap-x-3.5 gap-y-2 text-xs text-slate-400 [&_dd]:m-0 [&_dt]:text-slate-600">
                <dt>attempts</dt>
                <dd>
                  {retryTarget.attempts}/{retryTarget.maxAttempts}
                </dd>
                <dt>current status</dt>
                <dd>{retryTarget.status}</dd>
                <dt>operation</dt>
                <dd>reset to pending and make available now</dd>
              </dl>

              <Dialog.Description className="rounded-lg border border-amber-300/25 bg-amber-300/10 p-3 text-xs leading-5 text-amber-100">
                Retry is safe only when the handler is idempotent. Check
                downstream side effects before replaying this work.
              </Dialog.Description>
            </div>

            <footer className="flex justify-end gap-2.5 border-t border-white/10 p-4">
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
              <div className="mono mx-4 mb-4 rounded-lg border border-rose-300/30 bg-black/20 p-3 text-xs text-rose-100">
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
