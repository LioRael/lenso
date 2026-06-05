import { usePersistedLayout } from "../hooks/use-persisted-layout";

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

export function useOperationsInspectorLayout({
  defaultWidth,
  maxWidth,
  minWidth,
  storageKey,
}: {
  storageKey: string;
  defaultWidth: number;
  minWidth: number;
  maxWidth: number;
}) {
  const defaults = { inspectorWidth: defaultWidth };
  const [layout, setLayout, resetLayout] = usePersistedLayout(
    storageKey,
    defaults
  );
  const inspectorWidth = layout.inspectorWidth ?? defaultWidth;
  const resizeInspector = (deltaX: number) => {
    setLayout((current) => ({
      ...current,
      inspectorWidth: resizeOperationsInspectorWidth({
        currentWidth: current.inspectorWidth,
        defaultWidth,
        deltaX,
        maxWidth,
        minWidth,
      }),
    }));
  };

  return {
    inspectorWidth,
    resetLayout,
    resizeInspector,
  };
}
