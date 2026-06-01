import { useGSAP } from "@gsap/react";
import gsap from "gsap";
import {
  type CSSProperties,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

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

gsap.registerPlugin(useGSAP);

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
  const { activeTraceTarget, clearTraceTarget, openRetry } =
    useRuntimeConsole();
  const tracesQuery = useRuntimeTraces();
  const traces = tracesQuery.data ?? emptyTraces;
  const [query, setQuery] = useState("");
  const [selectedTraceId, setSelectedTraceId] = useState<string | null>(null);
  const [selectedSpanId, setSelectedSpanId] = useState<string | null>(null);
  const [displayedSpan, setDisplayedSpan] = useState<TraceSpan | null>(null);
  const [mode, setMode] = useState<TraceViewMode>("heatmap");
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("info");
  const workbenchRef = useRef<HTMLDivElement | null>(null);
  const inspectorPanelRef = useRef<HTMLDivElement | null>(null);
  const previousInspectorOpenRef = useRef(false);
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
  const inspectorOpen = selectedSpan !== null;
  const hasInspector = displayedSpan !== null;
  const listColumn = `clamp(220px,24vw,${traceLayout.listWidth}px)`;
  const inspectorColumn = `clamp(280px,30vw,${traceLayout.inspectorWidth}px)`;
  const gridTemplateColumns = hasInspector
    ? `${listColumn} 1px minmax(0,1fr) calc(1px * var(--trace-inspector-open)) minmax(0,calc(${inspectorColumn} * var(--trace-inspector-open)))`
    : `${listColumn} 1px minmax(0,1fr)`;

  useEffect(() => {
    if (selectedSpan) {
      setDisplayedSpan(selectedSpan);
    }
  }, [selectedSpan]);

  useGSAP(
    () => {
      const workbench = workbenchRef.current;
      const inspectorPanel = inspectorPanelRef.current;

      if (!workbench || (!displayedSpan && !previousInspectorOpenRef.current)) {
        return;
      }

      const reduceMotion = window.matchMedia(
        "(prefers-reduced-motion: reduce)"
      ).matches;
      const nextOpen = inspectorOpen ? 1 : 0;
      const hasOpenStateChanged =
        previousInspectorOpenRef.current !== inspectorOpen;
      previousInspectorOpenRef.current = inspectorOpen;
      gsap.killTweensOf(workbench);
      gsap.killTweensOf(inspectorPanel);

      if (!hasOpenStateChanged) {
        gsap.set(workbench, {
          "--trace-inspector-open": nextOpen,
        });
        gsap.set(inspectorPanel, {
          autoAlpha: nextOpen,
          x: inspectorOpen ? 0 : 18,
        });
        return;
      }

      if (reduceMotion) {
        gsap.set(workbench, {
          "--trace-inspector-open": nextOpen,
        });
        gsap.set(inspectorPanel, {
          autoAlpha: nextOpen,
          x: 0,
        });
        if (!inspectorOpen) {
          setDisplayedSpan(null);
        }
        return;
      }

      gsap.to(workbench, {
        "--trace-inspector-open": nextOpen,
        duration: inspectorOpen ? 0.32 : 0.24,
        ease: inspectorOpen ? "power3.out" : "power2.inOut",
        onComplete: () => {
          if (!inspectorOpen) {
            setDisplayedSpan(null);
          }
        },
      });
      gsap.fromTo(
        inspectorPanel,
        {
          autoAlpha: inspectorOpen ? 0 : 1,
          x: inspectorOpen ? 24 : 0,
        },
        {
          autoAlpha: inspectorOpen ? 1 : 0,
          duration: inspectorOpen ? 0.24 : 0.16,
          ease: inspectorOpen ? "power2.out" : "power2.in",
          x: inspectorOpen ? 0 : 18,
        }
      );
    },
    {
      dependencies: [
        displayedSpan?.id ?? null,
        inspectorOpen,
        traceLayout.inspectorWidth,
      ],
      scope: workbenchRef,
    }
  );

  const selectTrace = (trace: TraceRun) => {
    clearTraceTarget();
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
        280,
        560
      ),
    }));
  };

  const selectSpan = (span: TraceSpan) => {
    const ownerTrace = traces.find((trace) =>
      trace.spans.some((item) => item.id === span.id)
    );
    setSelectedTraceId(ownerTrace?.id ?? selectedTrace?.id ?? selectedTraceId);
    clearTraceTarget();
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
    <div className="h-full overflow-hidden bg-(--background) text-(--foreground)">
      <div
        ref={workbenchRef}
        className="grid h-full min-w-0 overflow-hidden"
        style={
          {
            "--trace-inspector-open": previousInspectorOpenRef.current ? 1 : 0,
            gridTemplateColumns,
          } as CSSProperties
        }
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

        <main className="grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden">
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

        {displayedSpan ? (
          <>
            <ResizeHandle
              ariaLabel="Resize trace inspector panel"
              onReset={resetLayout}
              onResize={resizeInspector}
            />

            <div
              ref={inspectorPanelRef}
              className="relative z-0 min-h-0 min-w-0 overflow-hidden"
              style={{
                pointerEvents: inspectorOpen ? "auto" : "none",
              }}
            >
              <TraceInspector
                activeTab={inspectorTab}
                onClearSelection={() => {
                  setSelectedTraceId(selectedTrace.id);
                  clearTraceTarget();
                  setSelectedSpanId(null);
                  setInspectorTab("info");
                }}
                selectedSpan={displayedSpan}
                setActiveTab={setInspectorTab}
                trace={selectedTrace}
              />
            </div>
          </>
        ) : null}
      </div>
    </div>
  );
}
