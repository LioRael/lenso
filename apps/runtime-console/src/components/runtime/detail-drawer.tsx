import { Activity, Copy, ExternalLink, RotateCcw, X } from "lucide-react";
import type { ReactNode } from "react";

import {
  retryTargetFor,
  type FunctionRun,
  type RuntimeEvent,
  type RuntimeRecord,
  type TimelineItem,
} from "../../data/mock-runtime";
import {
  useRuntimeEventDetail,
  useRuntimeFunctionDetail,
} from "../../hooks/use-runtime-queries";
import { actorLabel, duration, time } from "../../lib/format";
import { Button } from "../ui/button";
import { Drawer } from "../ui/drawer";
import { JsonViewer } from "./json-viewer";
import { useRuntimeConsole } from "./runtime-console-context";
import { StatusPill } from "./status-pill";

type DetailDrawerProps = {
  target: RuntimeRecord | null;
  onClose: () => void;
};

export function DetailDrawer({ target, onClose }: DetailDrawerProps) {
  return (
    <Drawer onOpenChange={(open) => !open && onClose()} open={Boolean(target)}>
      {target ? (
        <Drawer.Content aria-label="Runtime detail">
          <div className="flex items-start justify-between gap-4 border-b border-(--border-subtle) p-4.5">
            <div className="min-w-0">
              <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-(--muted)">
                {target.kind}
              </p>
              <Drawer.Title className="truncate text-lg font-semibold text-(--foreground)">
                {titleFor(target)}
              </Drawer.Title>
            </div>
            <Button
              aria-label="Close detail drawer"
              onClick={onClose}
              variant="ghost"
            >
              <X size={16} />
            </Button>
          </div>
          <div className="grid gap-3.5 p-4">{bodyFor(target)}</div>
        </Drawer.Content>
      ) : null}
    </Drawer>
  );
}

function titleFor(target: RuntimeRecord) {
  if (target.kind === "event") {
    return target.item.eventName;
  }
  if (target.kind === "function") {
    return target.item.functionName;
  }
  return target.item.name;
}

function bodyFor(target: RuntimeRecord) {
  if (target.kind === "event") {
    return <EventBody event={target.item} />;
  }
  if (target.kind === "function") {
    return <FunctionBody run={target.item} />;
  }
  return <TimelineBody item={target.item} record={target} />;
}

function SummaryStrip({
  attempts,
  durationValue,
  maxAttempts,
  status,
}: {
  status: RuntimeEvent["status"];
  attempts: number;
  maxAttempts: number;
  durationValue: string;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-3.5 gap-y-2 rounded-lg border border-(--border-subtle) bg-(--elevated) p-2.5 text-xs text-(--secondary)">
      <StatusPill status={status} />
      <span>
        attempts {attempts}/{maxAttempts}
      </span>
      <span>duration {durationValue}</span>
    </div>
  );
}

function EventBody({ event }: { event: RuntimeEvent }) {
  const { openRetry, openStoryTarget } = useRuntimeConsole();
  const detailQuery = useRuntimeEventDetail(event);
  const displayEvent = detailQuery.data ?? event;
  const displayRecord: RuntimeRecord = {
    kind: "event",
    item: displayEvent,
  };
  const retryTarget = retryTargetFor(displayRecord);
  return (
    <>
      <SummaryStrip
        attempts={displayEvent.attempts}
        durationValue={duration(
          displayEvent.lockedAt,
          displayEvent.publishedAt
        )}
        maxAttempts={displayEvent.maxAttempts}
        status={displayEvent.status}
      />
      {detailQuery.isFetching ? (
        <p className="text-xs text-(--muted)">Loading detail...</p>
      ) : null}
      {detailQuery.isError ? (
        <ErrorBox>
          Event detail unavailable: {errorMessage(detailQuery.error)}
        </ErrorBox>
      ) : null}
      <DrawerSection title="Metadata">
        <MetadataGrid>
          <dt>id</dt>
          <dd className="mono">{displayEvent.id}</dd>
          <dt>aggregate</dt>
          <dd className="mono">
            {displayEvent.aggregateType}:{displayEvent.aggregateId}
          </dd>
          <dt>source</dt>
          <dd className="mono">{displayEvent.sourceModule ?? "-"}</dd>
          <dt>version</dt>
          <dd>{displayEvent.eventVersion ?? "-"}</dd>
          <dt>locked by</dt>
          <dd className="mono">{displayEvent.lockedBy ?? "-"}</dd>
          <dt>created</dt>
          <dd>{time(displayEvent.createdAt)}</dd>
          <dt>occurred</dt>
          <dd>{time(displayEvent.occurredAt)}</dd>
          <dt>published</dt>
          <dd>{time(displayEvent.publishedAt)}</dd>
        </MetadataGrid>
      </DrawerSection>
      <DrawerSection title="Context">
        <MetadataGrid>
          <dt>correlation</dt>
          <dd className="mono">{displayEvent.correlationId}</dd>
          <dt>causation</dt>
          <dd className="mono">{displayEvent.causationId}</dd>
          <dt>actor</dt>
          <dd className="mono">{actorLabel(displayEvent.actor)}</dd>
        </MetadataGrid>
      </DrawerSection>
      {displayEvent.lastError ? (
        <DrawerSection title="Error">
          <ErrorBox>{displayEvent.lastError}</ErrorBox>
        </DrawerSection>
      ) : null}
      <JsonViewer title="Payload" value={displayEvent.payload} />
      {displayEvent.headers ? (
        <JsonViewer title="Headers" value={displayEvent.headers} />
      ) : null}
      {displayEvent.trace ? (
        <JsonViewer title="Trace" value={displayEvent.trace} />
      ) : null}
      <div className="flex flex-wrap gap-2.5">
        <Button
          onClick={() =>
            openStoryTarget({
              correlationId: displayEvent.correlationId,
              nodeIdCandidates: [displayEvent.id],
            })
          }
        >
          <ExternalLink size={15} />
          Timeline
        </Button>
        <Button variant="ghost">
          <Copy size={15} />
          Copy ID
        </Button>
        {retryTarget ? (
          <Button onClick={() => openRetry(retryTarget)} variant="danger">
            <RotateCcw size={15} />
            Retry
          </Button>
        ) : null}
      </div>
    </>
  );
}

function FunctionBody({ run }: { run: FunctionRun }) {
  const { openRetry, openStoryTarget } = useRuntimeConsole();
  const detailQuery = useRuntimeFunctionDetail(run);
  const displayRun = detailQuery.data ?? run;
  const displayRecord: RuntimeRecord = {
    kind: "function",
    item: displayRun,
  };
  const retryTarget = retryTargetFor(displayRecord);
  return (
    <>
      <SummaryStrip
        attempts={displayRun.attempts}
        durationValue={duration(displayRun.startedAt, displayRun.completedAt)}
        maxAttempts={displayRun.maxAttempts}
        status={displayRun.status}
      />
      {detailQuery.isFetching ? (
        <p className="text-xs text-(--muted)">Loading detail...</p>
      ) : null}
      {detailQuery.isError ? (
        <ErrorBox>
          Function detail unavailable: {errorMessage(detailQuery.error)}
        </ErrorBox>
      ) : null}
      <DrawerSection title="Metadata">
        <MetadataGrid>
          <dt>id</dt>
          <dd className="mono">{displayRun.id}</dd>
          <dt>locked by</dt>
          <dd className="mono">{displayRun.lockedBy ?? "-"}</dd>
          <dt>started</dt>
          <dd>{time(displayRun.startedAt)}</dd>
          <dt>completed</dt>
          <dd>{time(displayRun.completedAt)}</dd>
        </MetadataGrid>
      </DrawerSection>
      {displayRun.runtimeDeclaration ? (
        <DrawerSection title="Declaration">
          <MetadataGrid>
            <dt>module</dt>
            <dd className="mono">{displayRun.runtimeDeclaration.moduleName}</dd>
            <dt>source</dt>
            <dd>{displayRun.runtimeDeclaration.moduleSource}</dd>
            <dt>queue</dt>
            <dd className="mono">{displayRun.runtimeDeclaration.queue}</dd>
            <dt>version</dt>
            <dd>{displayRun.runtimeDeclaration.version}</dd>
            <dt>input schema</dt>
            <dd className="mono">
              {displayRun.runtimeDeclaration.inputSchema ?? "-"}
            </dd>
            <dt>retry policy</dt>
            <dd className="mono">
              {displayRun.runtimeDeclaration.retryPolicy
                ? `${displayRun.runtimeDeclaration.retryPolicy.maxAttempts} attempts / ${displayRun.runtimeDeclaration.retryPolicy.initialDelayMs}ms`
                : "-"}
            </dd>
          </MetadataGrid>
        </DrawerSection>
      ) : null}
      <DrawerSection title="Context">
        <MetadataGrid>
          <dt>correlation</dt>
          <dd className="mono">{displayRun.correlationId}</dd>
          <dt>actor</dt>
          <dd className="mono">{actorLabel(displayRun.actor)}</dd>
        </MetadataGrid>
      </DrawerSection>
      {displayRun.lastError ? (
        <DrawerSection title="Error">
          <ErrorBox>{displayRun.lastError}</ErrorBox>
        </DrawerSection>
      ) : null}
      <JsonViewer title="Input" value={displayRun.input} />
      {displayRun.output ? (
        <JsonViewer title="Output" value={displayRun.output} />
      ) : null}
      <DrawerSection title="Logs">
        <pre className="mono overflow-auto rounded-lg border border-(--border-subtle) bg-[color-mix(in_srgb,var(--background)_20%,transparent)] p-3 text-xs leading-6 text-(--secondary)">
          {displayRun.logs.join("\n")}
        </pre>
      </DrawerSection>
      <div className="flex flex-wrap gap-2.5">
        <Button
          onClick={() =>
            openStoryTarget({
              correlationId: displayRun.correlationId,
              nodeIdCandidates: [displayRun.id],
            })
          }
        >
          <Activity size={15} />
          Timeline
        </Button>
        {retryTarget ? (
          <Button onClick={() => openRetry(retryTarget)} variant="danger">
            <RotateCcw size={15} />
            Retry
          </Button>
        ) : null}
      </div>
    </>
  );
}

function TimelineBody({
  item,
  record,
}: {
  item: TimelineItem;
  record: RuntimeRecord;
}) {
  const { openRetry, openTimelineSource } = useRuntimeConsole();
  const retryTarget = retryTargetFor(record);
  return (
    <>
      <SummaryStrip
        attempts={item.attempts}
        durationValue={duration(item.startedAt, item.completedAt)}
        maxAttempts={item.maxAttempts}
        status={item.status}
      />
      <DrawerSection title="Metadata">
        <MetadataGrid>
          <dt>id</dt>
          <dd className="mono">{item.id}</dd>
          <dt>type</dt>
          <dd>{item.type}</dd>
          <dt>correlation</dt>
          <dd className="mono">{item.correlationId}</dd>
          <dt>created</dt>
          <dd>{time(item.createdAt)}</dd>
        </MetadataGrid>
      </DrawerSection>
      {item.lastError ? (
        <DrawerSection title="Error">
          <ErrorBox>{item.lastError}</ErrorBox>
        </DrawerSection>
      ) : null}
      <JsonViewer
        defaultExpanded
        title="Context"
        value={{
          id: item.id,
          type: item.type,
          correlation_id: item.correlationId,
          started_at: item.startedAt,
          completed_at: item.completedAt,
        }}
      />
      <div className="flex flex-wrap gap-2.5">
        <Button onClick={() => openTimelineSource(item)}>
          <ExternalLink size={15} />
          Open source record
        </Button>
        {retryTarget ? (
          <Button onClick={() => openRetry(retryTarget)} variant="danger">
            <RotateCcw size={15} />
            Retry
          </Button>
        ) : null}
      </div>
    </>
  );
}

function DrawerSection({
  children,
  title,
}: {
  children: ReactNode;
  title: string;
}) {
  return (
    <section className="grid gap-2">
      <h3 className="text-xs font-semibold text-(--secondary)">{title}</h3>
      {children}
    </section>
  );
}

function MetadataGrid({ children }: { children: ReactNode }) {
  return (
    <dl className="grid grid-cols-[120px_minmax(0,1fr)] gap-x-3.5 gap-y-2 text-xs text-(--secondary) [&_dd]:m-0 [&_dd]:min-w-0 [&_dt]:text-(--muted)">
      {children}
    </dl>
  );
}

function ErrorBox({ children }: { children: ReactNode }) {
  return (
    <div className="mono overflow-auto rounded-lg border border-[color-mix(in_srgb,var(--error)_30%,transparent)] bg-[color-mix(in_srgb,var(--error)_8%,transparent)] p-3 text-xs leading-6 text-(--error)">
      {children}
    </div>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Runtime request failed";
}
