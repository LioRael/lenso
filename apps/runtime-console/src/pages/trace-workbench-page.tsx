import { useMemo, useState } from "react";

import { ResizeHandle } from "../components/runtime/resize-handle";
import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { ServiceSummaryStrip } from "../components/runtime/service-summary-strip";
import { TraceHeader } from "../components/runtime/trace-header";
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
import { usePersistedLayout } from "../hooks/use-persisted-layout";
import { useRuntimeTraces } from "../hooks/use-runtime-queries";

type InspectorTab =
  | "info"
  | "attributes"
  | "events"
  | "errors"
  | "logs"
  | "context";

const emptyTraces: TraceRun[] = [];
const traceLayoutDefaults = {
  inspectorWidth: 376,
  listWidth: 340,
};

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function TraceWorkbenchPage() {
  const { activeTraceTarget, openRetry } = useRuntimeConsole();
  const tracesQuery = useRuntimeTraces();
  const traces = tracesQuery.data ?? emptyTraces;
  const [query, setQuery] = useState("");
  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null);
  const [selectedSpanId, setSelectedSpanId] = useState<string | null>(null);
  const [mode, setMode] = useState<TraceViewMode>("heatmap");
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("info");
  const [layout, setLayout, resetLayout] = usePersistedLayout(
    "runtime-console:traces-layout",
    traceLayoutDefaults
  );
  const traceLayout = { ...traceLayoutDefaults, ...layout };

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
    selectedTrace?.spans.find((span) => {
      const targetSpanId = activeTraceTarget?.spanId ?? selectedSpanId;
      return targetSpanId ? span.id === targetSpanId : false;
    }) ?? null;
  const selectedTraceIndex = Math.max(
    0,
    visibleTraces.findIndex((trace) => trace.id === selectedTrace?.id)
  );

  const selectTrace = (trace: TraceRun) => {
    setSelectedTraceId(trace.id);
    setSelectedSpanId(null);
    setInspectorTab("info");
  };

  const resizeTraceList = (deltaX: number) => {
    setLayout((current) => ({
      ...current,
      listWidth: clamp(
        (current.listWidth ?? traceLayoutDefaults.listWidth) + deltaX,
        220,
        420
      ),
    }));
  };

  const resizeInspector = (deltaX: number) => {
    setLayout((current) => ({
      ...current,
      inspectorWidth: clamp(
        (current.inspectorWidth ?? traceLayoutDefaults.inspectorWidth) - deltaX,
        320,
        560
      ),
    }));
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
    <div className="h-full overflow-hidden bg-[var(--background)] text-[var(--foreground)]">
      <div
        className="grid h-full min-w-0 overflow-hidden"
        style={{
          gridTemplateColumns: `${traceLayout.listWidth}px 4px minmax(0,1fr) 4px ${traceLayout.inspectorWidth}px`,
        }}
      >
        <TraceList
          onSelect={selectTrace}
          query={query}
          selectedTraceId={selectedTrace.id}
          setQuery={setQuery}
          traces={visibleTraces}
        />

        <ResizeHandle
          ariaLabel="Resize trace list panel"
          onReset={resetLayout}
          onResize={resizeTraceList}
        />

        <main className="grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden border-r border-[var(--border-subtle)]">
          <TraceHeader onSelectSpan={selectSpan} trace={selectedTrace} />

          <TraceVisualization
            mode={mode}
            onSelectSpan={selectSpan}
            selectedSpanId={selectedSpan?.id ?? null}
            setMode={setMode}
            trace={selectedTrace}
          />

          <ServiceSummaryStrip trace={selectedTrace} />
        </main>

        <ResizeHandle
          ariaLabel="Resize trace inspector panel"
          onReset={resetLayout}
          onResize={resizeInspector}
        />

        <div className="relative z-0 min-h-0 min-w-0 overflow-hidden">
          <TraceInspector
            activeTab={inspectorTab}
            onClearSelection={() => {
              setSelectedSpanId(null);
              setInspectorTab("info");
            }}
            selectedSpan={selectedSpan}
            setActiveTab={setInspectorTab}
            trace={selectedTrace}
          />
        </div>
      </div>
    </div>
  );
}
