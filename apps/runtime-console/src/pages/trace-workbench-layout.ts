export const traceLayoutDefaults = {
  inspectorWidth: 376,
  listWidth: 340,
  servicesHeight: 144,
};

export function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function resizeTraceListWidth(
  currentWidth: number | undefined,
  deltaX: number
) {
  return clamp(
    (currentWidth ?? traceLayoutDefaults.listWidth) + deltaX,
    220,
    420
  );
}

export const resizeStoryListWidth = resizeTraceListWidth;

export function resizeTraceInspectorWidth(
  currentWidth: number | undefined,
  deltaX: number
) {
  return clamp(
    (currentWidth ?? traceLayoutDefaults.inspectorWidth) - deltaX,
    280,
    560
  );
}

export function resizeTraceInspectorLayout({
  currentWidth,
  deltaX,
}: {
  currentWidth: number | undefined;
  deltaX: number;
}) {
  const width = currentWidth ?? traceLayoutDefaults.inspectorWidth;
  const nextWidth = resizeTraceInspectorWidth(currentWidth, deltaX);

  return {
    open: !(width <= 280 && deltaX > 0),
    width: nextWidth,
  };
}

export function resizeServicesPanelHeight(
  currentHeight: number | undefined,
  deltaY: number
) {
  return clamp(
    (currentHeight ?? traceLayoutDefaults.servicesHeight) - deltaY,
    112,
    360
  );
}

export function resizeServicesPanelLayout({
  currentHeight,
  deltaY,
  expanded,
}: {
  currentHeight: number | undefined;
  deltaY: number;
  expanded: boolean;
}) {
  const height = currentHeight ?? traceLayoutDefaults.servicesHeight;

  if (!expanded) {
    return {
      expanded: deltaY < 0,
      height,
    };
  }

  const nextHeight = resizeServicesPanelHeight(currentHeight, deltaY);

  return {
    expanded: !(height <= 112 && deltaY > 0),
    height: nextHeight,
  };
}
