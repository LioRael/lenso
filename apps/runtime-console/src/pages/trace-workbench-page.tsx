import { useMemo, useState } from "react";

import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { ServiceSummaryStrip } from "../components/runtime/service-summary-strip";
import { TraceInspector } from "../components/runtime/trace-inspector";
import { TraceList } from "../components/runtime/trace-list";
import type { TraceViewMode } from "../components/runtime/trace-tabs";
import { TraceVisualization } from "../components/runtime/trace-visualization";
import {
  isRetryable,
  type TraceRun,
  type TraceSpan,
} from "../data/mock-runtime";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import { useRuntimeTraces } from "../hooks/use-runtime-queries";
import { time } from "../lib/format";

type InspectorTab =
  | "info"
  | "attributes"
  | "events"
  | "errors"
  | "logs"
  | "context";

const emptyTraces: TraceRun[] = [];

export function TraceWorkbenchPage() {
  const { activeTraceTarget, openRetry } = useRuntimeConsole();
  const tracesQuery = useRuntimeTraces();
  const traces = tracesQuery.data ?? emptyTraces;
  const [query, setQuery] = useState("");
  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null);
  const [selectedSpanId, setSelectedSpanId] = useState<string | null>(null);
  const [mode, setMode] = useState<TraceViewMode>("waterfall");
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("info");

  const visibleTraces = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return traces.filter((trace) => {
      if (!normalized) {
        return true;
      }
      return [
        trace.id,
        trace.name,
        trace.service,
        trace.source,
        trace.correlationId,
      ].some((value) => value.toLowerCase().includes(normalized));
    });
  }, [query, traces]);

  const targetTrace = activeTraceTarget
    ? traces.find((trace) => trace.id === activeTraceTarget.traceId)
    : null;
  const selectedTrace =
    targetTrace ??
    traces.find((trace) => trace.id === selectedTraceId) ??
    visibleTraces[0] ??
    null;
  const selectedSpan =
    selectedTrace?.spans.find(
      (span) => span.id === (activeTraceTarget?.spanId ?? selectedSpanId)
    ) ??
    selectedTrace?.spans[0] ??
    null;
  const selectedTraceIndex = Math.max(
    0,
    visibleTraces.findIndex((trace) => trace.id === selectedTrace?.id)
  );

  const selectTrace = (trace: TraceRun) => {
    setSelectedTraceId(trace.id);
    setSelectedSpanId(trace.spans[0]?.id ?? null);
    setInspectorTab("info");
  };

  const selectSpan = (span: TraceSpan) => {
    setSelectedSpanId(span.id);
    setInspectorTab(
      span.status === "failed" || span.status === "dead" ? "errors" : "info"
    );
  };

  useListKeyboard({
    items: visibleTraces,
    onOpen: selectTrace,
    onRetry: (trace) => {
      const retryableSpan = trace.spans.find(
        (span) => isRetryable(span.status) && span.retryable
      );
      if (retryableSpan) {
        selectTrace(trace);
        selectSpan(retryableSpan);
        openRetry({
          attempts: retryableSpan.attempts ?? 1,
          id: retryableSpan.id,
          kind: "timeline",
          maxAttempts: retryableSpan.maxAttempts ?? 3,
          name: retryableSpan.name,
          status: retryableSpan.status,
        });
      }
    },
    selectedIndex: selectedTraceIndex,
    setSelectedIndex: (index) => {
      const trace = visibleTraces[index];
      if (trace) {
        selectTrace(trace);
      }
    },
  });

  if (tracesQuery.isLoading) {
    return (
      <div className="font-mono text-xs text-slate-500">loading traces...</div>
    );
  }

  if (tracesQuery.isError || !selectedTrace) {
    return (
      <div className="font-mono text-xs text-rose-300">
        trace workbench unavailable
      </div>
    );
  }

  return (
    <div className="h-[calc(100vh-68px)] overflow-hidden border border-white/10 bg-[#050609] shadow-2xl shadow-black/40">
      <div className="grid h-full grid-cols-[318px_minmax(0,1fr)_374px] max-xl:grid-cols-[280px_minmax(0,1fr)] max-lg:grid-cols-1">
        <TraceList
          onSelect={selectTrace}
          query={query}
          selectedTraceId={selectedTrace.id}
          setQuery={setQuery}
          traces={visibleTraces}
        />

        <main className="grid min-h-0 grid-rows-[48px_minmax(0,1fr)_auto]">
          <header className="border-b border-white/10 bg-[#07080a] px-2.5 py-2">
            <div className="flex items-center justify-between gap-4">
              <div className="min-w-0">
                <div className="truncate font-mono text-xs text-slate-100">
                  {selectedTrace.name}
                </div>
                <div className="mt-1 truncate font-mono text-[10px] text-slate-600">
                  {selectedTrace.id} · {selectedTrace.correlationId} ·{" "}
                  {time(selectedTrace.timestamp)}
                </div>
              </div>
              <div className="flex items-center gap-3 font-mono text-[10px] text-slate-500">
                <span>{selectedTrace.durationMs}ms</span>
                <span>{selectedTrace.spans.length} spans</span>
                <span
                  className={
                    isRetryable(selectedTrace.status)
                      ? "text-rose-300"
                      : "text-cyan-300"
                  }
                >
                  {selectedTrace.status}
                </span>
              </div>
            </div>
          </header>

          <TraceVisualization
            mode={mode}
            onSelectSpan={selectSpan}
            selectedSpanId={selectedSpan?.id ?? null}
            setMode={setMode}
            trace={selectedTrace}
          />

          <ServiceSummaryStrip trace={selectedTrace} />
        </main>

        <div className="max-xl:hidden">
          <TraceInspector
            activeTab={inspectorTab}
            selectedSpan={selectedSpan}
            setActiveTab={setInspectorTab}
            trace={selectedTrace}
          />
        </div>
      </div>
    </div>
  );
}
