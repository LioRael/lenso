import { Activity, Copy, ExternalLink, RotateCcw, X } from "lucide-react";
import type { ReactNode } from "react";

import {
  retryTargetFor,
  type FunctionRun,
  type RuntimeEvent,
  type RuntimeRecord,
  type TimelineItem,
} from "../../data/mock-runtime";
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
    return <EventBody event={target.item} record={target} />;
  }
  if (target.kind === "function") {
    return <FunctionBody record={target} run={target.item} />;
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

function EventBody({
  event,
  record,
}: {
  event: RuntimeEvent;
  record: RuntimeRecord;
}) {
  const { openRetry, openTimeline } = useRuntimeConsole();
  const retryTarget = retryTargetFor(record);
  return (
    <>
      <SummaryStrip
        attempts={event.attempts}
        durationValue={duration(event.lockedAt, event.publishedAt)}
        maxAttempts={event.maxAttempts}
        status={event.status}
      />
      <DrawerSection title="Metadata">
        <MetadataGrid>
          <dt>id</dt>
          <dd className="mono">{event.id}</dd>
          <dt>aggregate</dt>
          <dd className="mono">
            {event.aggregateType}:{event.aggregateId}
          </dd>
          <dt>created</dt>
          <dd>{time(event.createdAt)}</dd>
          <dt>published</dt>
          <dd>{time(event.publishedAt)}</dd>
        </MetadataGrid>
      </DrawerSection>
      <DrawerSection title="Context">
        <MetadataGrid>
          <dt>correlation</dt>
          <dd className="mono">{event.correlationId}</dd>
          <dt>causation</dt>
          <dd className="mono">{event.causationId}</dd>
          <dt>actor</dt>
          <dd className="mono">{actorLabel(event.actor)}</dd>
        </MetadataGrid>
      </DrawerSection>
      {event.lastError ? (
        <DrawerSection title="Error">
          <ErrorBox>{event.lastError}</ErrorBox>
        </DrawerSection>
      ) : null}
      <JsonViewer title="Payload" value={event.payload} />
      <div className="flex flex-wrap gap-2.5">
        <Button onClick={() => openTimeline(event.correlationId)}>
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

function FunctionBody({
  record,
  run,
}: {
  run: FunctionRun;
  record: RuntimeRecord;
}) {
  const { openRetry, openTimeline } = useRuntimeConsole();
  const retryTarget = retryTargetFor(record);
  return (
    <>
      <SummaryStrip
        attempts={run.attempts}
        durationValue={duration(run.startedAt, run.completedAt)}
        maxAttempts={run.maxAttempts}
        status={run.status}
      />
      <DrawerSection title="Metadata">
        <MetadataGrid>
          <dt>id</dt>
          <dd className="mono">{run.id}</dd>
          <dt>locked by</dt>
          <dd className="mono">{run.lockedBy ?? "-"}</dd>
          <dt>started</dt>
          <dd>{time(run.startedAt)}</dd>
          <dt>completed</dt>
          <dd>{time(run.completedAt)}</dd>
        </MetadataGrid>
      </DrawerSection>
      <DrawerSection title="Context">
        <MetadataGrid>
          <dt>correlation</dt>
          <dd className="mono">{run.correlationId}</dd>
          <dt>actor</dt>
          <dd className="mono">{actorLabel(run.actor)}</dd>
        </MetadataGrid>
      </DrawerSection>
      {run.lastError ? (
        <DrawerSection title="Error">
          <ErrorBox>{run.lastError}</ErrorBox>
        </DrawerSection>
      ) : null}
      <JsonViewer title="Input" value={run.input} />
      {run.output ? <JsonViewer title="Output" value={run.output} /> : null}
      <DrawerSection title="Logs">
        <pre className="mono overflow-auto rounded-lg border border-(--border-subtle) bg-[color-mix(in_srgb,var(--background)_20%,transparent)] p-3 text-xs leading-6 text-(--secondary)">
          {run.logs.join("\n")}
        </pre>
      </DrawerSection>
      <div className="flex flex-wrap gap-2.5">
        <Button onClick={() => openTimeline(run.correlationId)}>
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
  const { openRetry } = useRuntimeConsole();
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
        <Button>
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
