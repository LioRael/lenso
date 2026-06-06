export const runtimeStoriesLayoutDefaults = {
  inspectorWidth: 376,
  listWidth: 340,
  servicesHeight: 144,
};

export function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function resizeStoryListWidth(
  currentWidth: number | undefined,
  deltaX: number
) {
  return clamp(
    (currentWidth ?? runtimeStoriesLayoutDefaults.listWidth) + deltaX,
    220,
    420
  );
}

export function resizeExecutionInspectorWidth(
  currentWidth: number | undefined,
  deltaX: number
) {
  return clamp(
    (currentWidth ?? runtimeStoriesLayoutDefaults.inspectorWidth) - deltaX,
    280,
    560
  );
}

export function resizeExecutionInspectorLayout({
  currentWidth,
  deltaX,
}: {
  currentWidth: number | undefined;
  deltaX: number;
}) {
  const width = currentWidth ?? runtimeStoriesLayoutDefaults.inspectorWidth;
  const nextWidth = resizeExecutionInspectorWidth(currentWidth, deltaX);

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
    (currentHeight ?? runtimeStoriesLayoutDefaults.servicesHeight) - deltaY,
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
  const height = currentHeight ?? runtimeStoriesLayoutDefaults.servicesHeight;

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
