import { RotateCcw } from "lucide-react";

import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { JsonViewer } from "./json-viewer";
import { useRuntimeConsole } from "./runtime-console-context";

type InspectorTab =
  | "info"
  | "attributes"
  | "events"
  | "errors"
  | "logs"
  | "context";

const tabs: InspectorTab[] = [
  "info",
  "attributes",
  "events",
  "errors",
  "logs",
  "context",
];

export function TraceInspector({
  activeTab,
  selectedSpan,
  setActiveTab,
  trace,
}: {
  trace: TraceRun;
  selectedSpan: TraceSpan | null;
  activeTab: InspectorTab;
  setActiveTab: (tab: InspectorTab) => void;
}) {
  const span = selectedSpan ?? trace.spans[0] ?? null;

  return (
    <aside className="min-h-0 border-l border-white/10 bg-[#07080a]">
      <div className="border-b border-white/10 px-2.5 py-2">
        <div className="font-mono text-[10px] uppercase tracking-[0.05em] text-slate-600">
          inspector
        </div>
        <div className="mt-1 truncate font-mono text-xs text-slate-200">
          {span?.name ?? trace.name}
        </div>
      </div>
      <div className="flex h-7 border-b border-white/10">
        {tabs.map((tab) => (
          <button
            className={cn(
              "border-r border-white/10 px-2 font-mono text-[10px] text-slate-600 hover:bg-white/[0.04] hover:text-slate-300",
              activeTab === tab && "bg-cyan-300/[0.06] text-cyan-200"
            )}
            key={tab}
            onClick={() => setActiveTab(tab)}
          >
            {tab}
          </button>
        ))}
      </div>
      <div className="h-[calc(100%-68px)] overflow-auto p-2.5">
        {span ? (
          <InspectorBody activeTab={activeTab} span={span} trace={trace} />
        ) : (
          <div className="font-mono text-xs text-slate-600">
            no span selected
          </div>
        )}
      </div>
    </aside>
  );
}

function InspectorBody({
  activeTab,
  span,
  trace,
}: {
  trace: TraceRun;
  span: TraceSpan;
  activeTab: InspectorTab;
}) {
  const { openRetry } = useRuntimeConsole();

  if (activeTab === "info") {
    return (
      <div className="grid gap-3 font-mono text-[11px]">
        <dl className="grid grid-cols-[86px_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-slate-600">
          <dt>trace</dt>
          <dd className="truncate text-slate-300">{trace.id}</dd>
          <dt>span</dt>
          <dd className="truncate text-slate-300">{span.id}</dd>
          <dt>service</dt>
          <dd className="text-slate-300">{span.service}</dd>
          <dt>kind</dt>
          <dd className="text-slate-300">{span.kind}</dd>
          <dt>status</dt>
          <dd
            className={
              span.status === "failed" || span.status === "dead"
                ? "text-rose-300"
                : "text-cyan-300"
            }
          >
            {span.status}
          </dd>
          <dt>duration</dt>
          <dd className="text-slate-300">{span.durationMs}ms</dd>
        </dl>
        {span.retryable ? (
          <button
            className="inline-flex h-7 w-fit items-center gap-2 border border-rose-300/30 bg-rose-300/10 px-2 text-[11px] text-rose-100 hover:bg-rose-300/15"
            onClick={() =>
              openRetry({
                attempts: span.attempts ?? 1,
                id: span.id,
                kind: "timeline",
                maxAttempts: span.maxAttempts ?? 3,
                name: span.name,
                status: span.status,
              })
            }
          >
            <RotateCcw size={12} />
            Retry runtime item
          </button>
        ) : null}
        {span.payload ? (
          <JsonViewer defaultExpanded title="Payload" value={span.payload} />
        ) : null}
      </div>
    );
  }

  if (activeTab === "attributes") {
    return (
      <JsonViewer defaultExpanded title="Attributes" value={span.attributes} />
    );
  }

  if (activeTab === "events") {
    return <JsonViewer defaultExpanded title="Events" value={span.events} />;
  }

  if (activeTab === "errors") {
    const errors =
      span.status === "failed" || span.status === "dead" ? span.logs : [];
    return <JsonViewer defaultExpanded title="Errors" value={errors} />;
  }

  if (activeTab === "logs") {
    return (
      <pre className="font-mono text-[11px] leading-5 text-slate-300">
        {span.logs.join("\n")}
      </pre>
    );
  }

  return <JsonViewer defaultExpanded title="Context" value={span.context} />;
}
