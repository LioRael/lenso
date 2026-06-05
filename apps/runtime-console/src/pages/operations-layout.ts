export type OperationsInspectorLayout = {
  inspectorWidth: number;
};

export function resizeOperationsInspectorWidth({
  currentWidth,
  defaultWidth,
  deltaX,
  maxWidth,
  minWidth,
}: {
  currentWidth: number | undefined;
  defaultWidth: number;
  deltaX: number;
  minWidth: number;
  maxWidth: number;
}) {
  const width = (currentWidth ?? defaultWidth) - deltaX;
  return Math.min(maxWidth, Math.max(minWidth, width));
}
