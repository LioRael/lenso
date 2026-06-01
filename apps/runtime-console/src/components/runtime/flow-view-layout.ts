export const flowViewDefaults = {
  fitPadding: 128,
  maxZoom: 1.8,
  minZoom: 0.55,
  zoomStep: 0.15,
} as const;

export function clampFlowZoom(zoom: number) {
  return Math.min(
    flowViewDefaults.maxZoom,
    Math.max(flowViewDefaults.minZoom, roundZoom(zoom))
  );
}

export function getFitToFrameZoom({
  canvasHeight,
  canvasWidth,
  viewportHeight,
  viewportWidth,
}: {
  canvasHeight: number;
  canvasWidth: number;
  viewportHeight: number;
  viewportWidth: number;
}) {
  const availableWidth = Math.max(
    1,
    viewportWidth - flowViewDefaults.fitPadding
  );
  const availableHeight = Math.max(
    1,
    viewportHeight - flowViewDefaults.fitPadding
  );
  return clampFlowZoom(
    Math.min(availableWidth / canvasWidth, availableHeight / canvasHeight)
  );
}

export function getWorkspaceLayout({
  canvasHeight,
  canvasWidth,
  viewportHeight,
  viewportWidth,
  zoom,
}: {
  canvasHeight: number;
  canvasWidth: number;
  viewportHeight: number;
  viewportWidth: number;
  zoom: number;
}) {
  const marginLeft = Math.round(viewportWidth / 2);
  const marginTop = Math.round(viewportHeight / 2);
  const scaledCanvasWidth = canvasWidth * zoom;
  const scaledCanvasHeight = canvasHeight * zoom;

  return {
    marginLeft,
    marginTop,
    scaledCanvasHeight,
    scaledCanvasWidth,
    workspaceHeight: Math.round(scaledCanvasHeight + marginTop * 2),
    workspaceWidth: Math.round(scaledCanvasWidth + marginLeft * 2),
  };
}

export function getFrameScrollPosition({
  bounds,
  marginLeft,
  marginTop,
  viewportHeight,
  viewportWidth,
  zoom,
}: {
  bounds: FlowBounds;
  marginLeft: number;
  marginTop: number;
  viewportHeight: number;
  viewportWidth: number;
  zoom: number;
}) {
  return {
    scrollLeft: Math.max(
      0,
      roundPosition(
        marginLeft + (bounds.x + bounds.width / 2) * zoom - viewportWidth / 2
      )
    ),
    scrollTop: Math.max(
      0,
      roundPosition(
        marginTop + (bounds.y + bounds.height / 2) * zoom - viewportHeight / 2
      )
    ),
  };
}

export type FlowBounds = {
  height: number;
  width: number;
  x: number;
  y: number;
};

export function getNodeBounds(
  nodes: Array<{ x: number; y: number }>,
  nodeWidth = 240,
  nodeHeight = 72
): FlowBounds {
  if (nodes.length === 0) {
    return { height: nodeHeight, width: nodeWidth, x: 0, y: 0 };
  }
  const minX = Math.min(...nodes.map((node) => node.x));
  const minY = Math.min(...nodes.map((node) => node.y));
  const maxX = Math.max(...nodes.map((node) => node.x + nodeWidth));
  const maxY = Math.max(...nodes.map((node) => node.y + nodeHeight));

  return {
    height: maxY - minY,
    width: maxX - minX,
    x: minX,
    y: minY,
  };
}

export function getZoomAroundPoint({
  currentZoom,
  nextZoom,
  pointerX,
  pointerY,
  scrollLeft,
  scrollTop,
}: {
  currentZoom: number;
  nextZoom: number;
  pointerX: number;
  pointerY: number;
  scrollLeft: number;
  scrollTop: number;
}) {
  const zoom = clampFlowZoom(nextZoom);
  const scale = zoom / currentZoom;

  return {
    scrollLeft: roundPosition((scrollLeft + pointerX) * scale - pointerX),
    scrollTop: roundPosition((scrollTop + pointerY) * scale - pointerY),
    zoom,
  };
}

function roundPosition(position: number) {
  return Math.round(position);
}

function roundZoom(zoom: number) {
  return Math.round(zoom * 100) / 100;
}
