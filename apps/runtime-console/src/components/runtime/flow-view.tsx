import { Maximize2, Minus, Plus } from "lucide-react";
import type { PointerEvent, WheelEvent } from "react";
import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";

import type { RuntimeStory, ExecutionNode } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { formatRuntimeDuration, serviceColor } from "../../lib/runtime-style";
import {
  clampFlowZoom,
  flowViewDefaults,
  getFitToFrameZoom,
  getFrameScrollPosition,
  getNodeBounds,
  getWorkspaceLayout,
  getZoomAroundPoint,
} from "./flow-view-layout";
import { buildParallelExecutionGroups } from "./parallel-execution-model";
import {
  buildRuntimeGraphLayout,
  buildRuntimeGraphModel,
} from "./runtime-graph-model";
import { RuntimeViewHeader } from "./runtime-view-header";

const nodeWidth = 240;
const nodeHeight = 72;
const columnWidth = 280;
const rowHeight = 92;
const canvasPadding = 64;
const minimapWidth = 140;
const minimapHeight = 100;

export function FlowView({
  selectedNodeId,
  story,
  onSelectNode,
}: {
  story: RuntimeStory;
  selectedNodeId: string | null;
  onSelectNode: (node: ExecutionNode) => void;
}) {
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const panRef = useRef<{
    pointerId: number;
    scrollLeft: number;
    scrollTop: number;
    x: number;
    y: number;
  } | null>(null);
  const hasCenteredRef = useRef(false);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  const [isPanning, setIsPanning] = useState(false);
  const [viewportSize, setViewportSize] = useState({ height: 0, width: 0 });
  const [zoom, setZoom] = useState(1);

  const graphModel = useMemo(() => buildRuntimeGraphModel(story), [story]);
  const { edges } = graphModel;
  const graphLayout = useMemo(() => buildRuntimeGraphLayout(story), [story]);
  const parallelGroups = useMemo(
    () => buildParallelExecutionGroups(story),
    [story]
  );
  const parallelGroupByParent = useMemo(
    () => new Map(parallelGroups.map((group) => [group.parentId, group])),
    [parallelGroups]
  );
  const parallelGroupByChild = useMemo(
    () =>
      new Map(
        parallelGroups.flatMap((group) =>
          group.childIds.map((childId) => [childId, group] as const)
        )
      ),
    [parallelGroups]
  );
  const nodes = useMemo(
    () =>
      graphLayout.nodes.map((layoutNode) => ({
        fanoutGroup: parallelGroupByParent.get(layoutNode.node.id),
        node: layoutNode.node,
        parallelGroup: parallelGroupByChild.get(layoutNode.node.id),
        parentId: layoutNode.parentId,
        x: layoutNode.depth * columnWidth,
        y: layoutNode.row * rowHeight,
      })),
    [graphLayout.nodes, parallelGroupByChild, parallelGroupByParent]
  );
  const nodesById = useMemo(
    () => new Map(nodes.map((node) => [node.node.id, node])),
    [nodes]
  );
  const nodeBounds = useMemo(
    () => getNodeBounds(nodes, nodeWidth, nodeHeight),
    [nodes]
  );
  const canvasWidth = Math.max(
    980,
    Math.max(0, ...nodes.map((node) => node.x + nodeWidth)) + canvasPadding * 2
  );
  const canvasHeight = Math.max(420, nodes.length * rowHeight);
  const workspaceLayout = getWorkspaceLayout({
    canvasHeight,
    canvasWidth,
    viewportHeight: viewportSize.height,
    viewportWidth: viewportSize.width,
    zoom,
  });
  const minimapScale = Math.min(
    (minimapWidth - 16) / canvasWidth,
    (minimapHeight - 16) / canvasHeight
  );

  const centerGraph = useCallback(
    (nextZoom: number) => {
      const viewport = viewportRef.current;
      if (!viewport) {
        return;
      }
      const layout = getWorkspaceLayout({
        canvasHeight,
        canvasWidth,
        viewportHeight: viewport.clientHeight,
        viewportWidth: viewport.clientWidth,
        zoom: nextZoom,
      });
      const position = getFrameScrollPosition({
        bounds: nodeBounds,
        marginLeft: layout.marginLeft,
        marginTop: layout.marginTop,
        viewportHeight: viewport.clientHeight,
        viewportWidth: viewport.clientWidth,
        zoom: nextZoom,
      });
      requestAnimationFrame(() => {
        viewport.scrollTo({
          left: position.scrollLeft,
          top: position.scrollTop,
        });
      });
    },
    [canvasHeight, canvasWidth, nodeBounds]
  );

  const setViewportNode = useCallback((node: HTMLDivElement | null) => {
    resizeObserverRef.current?.disconnect();
    resizeObserverRef.current = null;
    viewportRef.current = node;
    if (!node) {
      return;
    }
    const updateViewportSize = () => {
      setViewportSize({
        height: node.clientHeight,
        width: node.clientWidth,
      });
    };
    const observer = new ResizeObserver(updateViewportSize);
    updateViewportSize();
    observer.observe(node);
    resizeObserverRef.current = observer;
  }, []);

  useLayoutEffect(() => {
    if (
      hasCenteredRef.current ||
      viewportSize.height <= 0 ||
      viewportSize.width <= 0
    ) {
      return;
    }
    hasCenteredRef.current = true;
    centerGraph(zoom);
  }, [centerGraph, viewportSize.height, viewportSize.width, zoom]);

  const frameCanvas = useCallback(() => {
    const viewport = viewportRef.current;
    if (!viewport) {
      return;
    }
    const nextZoom = getFitToFrameZoom({
      canvasHeight,
      canvasWidth,
      viewportHeight: viewport.clientHeight,
      viewportWidth: viewport.clientWidth,
    });
    setZoom(nextZoom);
    centerGraph(nextZoom);
  }, [canvasHeight, canvasWidth, centerGraph]);

  const zoomBy = useCallback((delta: number) => {
    const viewport = viewportRef.current;
    if (!viewport) {
      setZoom((current) => clampFlowZoom(current + delta));
      return;
    }
    setZoom((current) => {
      const result = getZoomAroundPoint({
        currentZoom: current,
        nextZoom: current + delta,
        pointerX: viewport.clientWidth / 2,
        pointerY: viewport.clientHeight / 2,
        scrollLeft: viewport.scrollLeft,
        scrollTop: viewport.scrollTop,
      });
      requestAnimationFrame(() => {
        viewport.scrollTo({
          left: result.scrollLeft,
          top: result.scrollTop,
        });
      });
      return result.zoom;
    });
  }, []);

  const handleWheel = (event: WheelEvent<HTMLDivElement>) => {
    if (!(event.metaKey || event.ctrlKey)) {
      return;
    }
    event.preventDefault();
    const viewport = viewportRef.current;
    if (!viewport) {
      return;
    }
    const rect = viewport.getBoundingClientRect();
    setZoom((current) => {
      const result = getZoomAroundPoint({
        currentZoom: current,
        nextZoom: current + (event.deltaY > 0 ? -0.08 : 0.08),
        pointerX: event.clientX - rect.left,
        pointerY: event.clientY - rect.top,
        scrollLeft: viewport.scrollLeft,
        scrollTop: viewport.scrollTop,
      });
      requestAnimationFrame(() => {
        viewport.scrollTo({
          left: result.scrollLeft,
          top: result.scrollTop,
        });
      });
      return result.zoom;
    });
  };

  const startPan = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }
    const target = event.target as HTMLElement;
    if (target.closest("button")) {
      return;
    }
    const viewport = viewportRef.current;
    if (!viewport) {
      return;
    }
    panRef.current = {
      pointerId: event.pointerId,
      scrollLeft: viewport.scrollLeft,
      scrollTop: viewport.scrollTop,
      x: event.clientX,
      y: event.clientY,
    };
    viewport.setPointerCapture(event.pointerId);
    setIsPanning(true);
  };

  const panCanvas = (event: PointerEvent<HTMLDivElement>) => {
    const pan = panRef.current;
    const viewport = viewportRef.current;
    if (!pan || !viewport || pan.pointerId !== event.pointerId) {
      return;
    }
    viewport.scrollLeft = pan.scrollLeft - (event.clientX - pan.x);
    viewport.scrollTop = pan.scrollTop - (event.clientY - pan.y);
  };

  const stopPan = (event: PointerEvent<HTMLDivElement>) => {
    const viewport = viewportRef.current;
    const pan = panRef.current;
    if (viewport && pan?.pointerId === event.pointerId) {
      viewport.releasePointerCapture(event.pointerId);
    }
    panRef.current = null;
    setIsPanning(false);
  };

  return (
    <div className="isolate relative h-full min-w-0 overflow-hidden bg-(--sidebar)">
      <div className="absolute top-0 right-0 left-0 z-2">
        <RuntimeViewHeader
          summary={`${nodes.length} nodes · ${edges.length} ${graphModel.source === "backend" ? "backend" : "derived"} edges · ${Math.round(zoom * 100)}%`}
          title="Execution Graph"
        >
          <button
            className="flex items-center gap-1.5 transition hover:text-(--foreground)"
            onClick={frameCanvas}
            type="button"
          >
            <Maximize2 size={12} />
            Frame
          </button>
        </RuntimeViewHeader>
      </div>

      {graphModel.state === "missing-edges" ? (
        <div className="absolute top-12 left-1/2 z-3 w-[min(520px,calc(100%-32px))] -translate-x-1/2 border tint-border tint-warning bg-[color-mix(in_srgb,var(--background)_92%,transparent)] p-3 font-mono text-[11px] tint-text shadow-(--elevation-overlay)">
          This story includes execution nodes, but the backend did not return
          graph edges.
        </div>
      ) : null}

      {graphModel.state === "empty-nodes" ? (
        <div className="absolute inset-0 z-3 grid place-items-center p-4">
          <div className="border border-(--border-subtle) bg-(--surface) p-4 font-mono text-xs text-(--muted)">
            This story does not include execution nodes yet.
          </div>
        </div>
      ) : null}

      <div
        className={cn(
          "relative z-0 h-full overflow-auto",
          isPanning ? "cursor-grabbing select-none" : "cursor-grab"
        )}
        onPointerCancel={stopPan}
        onPointerDown={startPan}
        onPointerMove={panCanvas}
        onPointerUp={stopPan}
        onWheel={handleWheel}
        ref={setViewportNode}
      >
        <div
          className="relative"
          style={{
            height: workspaceLayout.workspaceHeight,
            width: workspaceLayout.workspaceWidth,
          }}
        >
          <div
            className="absolute top-0 left-0"
            style={{
              height: canvasHeight,
              left: workspaceLayout.marginLeft,
              top: workspaceLayout.marginTop,
              transform: `scale(${zoom})`,
              transformOrigin: "top left",
              width: canvasWidth,
            }}
          >
            <svg
              aria-label="Story flow connectors"
              className="pointer-events-none absolute inset-0 size-full"
            >
              <title>Story flow connectors</title>
              {edges.map((edge) => {
                const source = nodesById.get(edge.source);
                const target = nodesById.get(edge.target);
                if (!source || !target) {
                  return null;
                }
                const fromX = source.x + nodeWidth;
                const fromY = source.y + nodeHeight / 2;
                const toX = target.x;
                const toY = target.y + nodeHeight / 2;
                const midX = (fromX + toX) / 2;
                return (
                  <path
                    d={`M ${fromX} ${fromY} C ${midX} ${fromY}, ${midX} ${toY}, ${toX} ${toY}`}
                    fill="none"
                    key={edge.id}
                    opacity="0.72"
                    stroke="var(--muted-deep)"
                    strokeDasharray={edge.type === "causation" ? "none" : "6 4"}
                    strokeWidth="1.5"
                  />
                );
              })}
            </svg>

            {nodes.map(({ fanoutGroup, node, parallelGroup, x, y }) => {
              const color = serviceColor(node.service);
              const isSelected = selectedNodeId === node.id;
              const isError =
                node.status === "failed" || node.status === "dead";
              return (
                <button
                  aria-label={`Select graph node ${node.name}`}
                  className={cn(
                    "absolute h-18 w-60 cursor-pointer rounded-sm border bg-(--elevated) text-left transition hover:bg-(--hover)",
                    isSelected &&
                      "border-(--accent) shadow-[0_0_12px_color-mix(in_srgb,var(--accent)_22%,transparent)] ring-1 ring-[color-mix(in_srgb,var(--accent)_30%,transparent)]",
                    !isSelected &&
                      isError &&
                      "border-[color-mix(in_srgb,var(--error)_45%,transparent)]",
                    !isSelected &&
                      !isError &&
                      "border-(--border-subtle) hover:border-(--muted-deep)"
                  )}
                  key={node.id}
                  onClick={() => onSelectNode(node)}
                  style={{ left: x, top: y }}
                  type="button"
                >
                  <span
                    className="absolute top-0 right-0 left-0 h-0.75 rounded-t-sm"
                    style={{ backgroundColor: color }}
                  />
                  <span className="flex h-full flex-col justify-between px-3 pt-2.5 pb-2">
                    <span className="flex items-start justify-between gap-2">
                      <span
                        className="rounded-xs border px-1.5 py-0.5 font-mono text-[10px] font-bold uppercase tracking-[0.06em]"
                        style={{
                          backgroundColor: `${color}18`,
                          borderColor: `${color}30`,
                          color,
                        }}
                      >
                        {node.service}
                      </span>
                      <span
                        className={cn(
                          "rounded-xs px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-[0.06em]",
                          isError
                            ? "bg-[color-mix(in_srgb,var(--error)_10%,transparent)] text-(--error)"
                            : "bg-[color-mix(in_srgb,var(--accent)_10%,transparent)] text-(--accent)"
                        )}
                      >
                        {node.kind}
                      </span>
                    </span>
                    <span className="min-w-0">
                      <span className="block truncate font-mono text-[13px] text-(--foreground)">
                        {node.name}
                      </span>
                      <span className="mt-1 flex min-w-0 items-center gap-1.5 font-mono text-[11px] text-(--muted)">
                        <span>{formatRuntimeDuration(node.durationMs)}</span>
                        {fanoutGroup ? (
                          <span className="shrink-0 rounded-xs px-1 py-0 text-[10px] tint tint-info">
                            fan-out {fanoutGroup.branchCount}
                          </span>
                        ) : null}
                        {!fanoutGroup && parallelGroup ? (
                          <span className="shrink-0 rounded-xs px-1 py-0 text-[10px] tint tint-info">
                            parallel
                          </span>
                        ) : null}
                      </span>
                    </span>
                  </span>
                  {isError ? (
                    <span className="absolute -top-1 -right-1 size-2.5 rounded-full border border-(--elevated) bg-[#ef4444]" />
                  ) : null}
                </button>
              );
            })}
          </div>
        </div>
      </div>

      <div className="absolute bottom-10 left-4 z-2 flex flex-col gap-1">
        <button
          aria-label="Zoom graph in"
          className="grid size-7 place-items-center rounded-xs border border-(--border-subtle) bg-(--elevated) text-(--secondary) transition hover:border-(--muted-deep) hover:text-(--foreground)"
          onClick={() => zoomBy(flowViewDefaults.zoomStep)}
          type="button"
        >
          <Plus size={14} />
        </button>
        <button
          aria-label="Zoom graph out"
          className="grid size-7 place-items-center rounded-xs border border-(--border-subtle) bg-(--elevated) text-(--secondary) transition hover:border-(--muted-deep) hover:text-(--foreground)"
          onClick={() => zoomBy(-flowViewDefaults.zoomStep)}
          type="button"
        >
          <Minus size={14} />
        </button>
        <button
          aria-label="Frame graph"
          className="grid size-7 place-items-center rounded-xs border border-(--border-subtle) bg-(--elevated) text-(--secondary) transition hover:border-(--muted-deep) hover:text-(--foreground)"
          onClick={frameCanvas}
          type="button"
        >
          <Maximize2 size={14} />
        </button>
      </div>

      <div className="absolute right-4 bottom-10 z-2 h-25 w-35 overflow-hidden rounded-xs border border-(--border-subtle) bg-[color-mix(in_srgb,var(--background)_90%,transparent)]">
        <div
          className="absolute top-2 left-2"
          style={{
            height: canvasHeight,
            transform: `scale(${minimapScale})`,
            transformOrigin: "top left",
            width: canvasWidth,
          }}
        >
          {nodes.map(({ node, x, y }) => (
            <div
              className="absolute h-18 w-60 rounded-sm"
              key={node.id}
              style={{
                backgroundColor: serviceColor(node.service),
                left: x,
                opacity: selectedNodeId === node.id ? 1 : 0.45,
                top: y,
              }}
            />
          ))}
        </div>
      </div>

      <div className="absolute bottom-2 left-1/2 z-2 flex -translate-x-1/2 items-center gap-4 rounded-xs border border-(--border-subtle) bg-[color-mix(in_srgb,var(--background)_84%,transparent)] px-3 py-1.5 font-mono text-[11px] text-(--muted)">
        <span>Select nodes</span>
        <span>{Math.round(zoom * 100)}%</span>
        <span>Ctrl wheel zoom</span>
        <span>Drag canvas</span>
      </div>
    </div>
  );
}
