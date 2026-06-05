import { useCallback, useEffect, useMemo } from "react";

import {
  pushOperationsUrl,
  replaceOperationsUrl,
} from "./operations-url-model";

export function selectedOperationItem<T>({
  getId,
  items,
  selectedId,
}: {
  items: T[];
  selectedId: string;
  getId: (item: T) => string;
}) {
  return items.find((item) => getId(item) === selectedId) ?? null;
}

export function selectedOperationIndex<T>({
  getId,
  items,
  selectedId,
}: {
  items: T[];
  selectedId: string;
  getId: (item: T) => string;
}) {
  const index = items.findIndex((item) => getId(item) === selectedId);
  return Math.max(0, index);
}

export function nextOperationSelectedId<T>({
  getId,
  items,
  selectedId,
}: {
  items: T[];
  selectedId: string;
  getId: (item: T) => string;
}) {
  if (items.length === 0) {
    return "";
  }
  return items.some((item) => getId(item) === selectedId)
    ? selectedId
    : getId(items[0]!);
}

export function useOperationsSelection<T>({
  currentPath,
  getId,
  items,
  pathForSelectedId,
  selectedId,
  setSelectedId,
}: {
  items: T[];
  selectedId: string;
  setSelectedId: (selectedId: string) => void;
  getId: (item: T) => string;
  currentPath: string;
  pathForSelectedId: (selectedId: string) => string;
}) {
  useEffect(() => {
    const nextSelectedId = nextOperationSelectedId({
      getId,
      items,
      selectedId,
    });
    if (nextSelectedId !== selectedId) {
      setSelectedId(nextSelectedId);
    }
  }, [getId, items, selectedId, setSelectedId]);

  useEffect(() => {
    replaceOperationsUrl(currentPath);
  }, [currentPath]);

  const selected = useMemo(
    () => selectedOperationItem({ getId, items, selectedId }),
    [getId, items, selectedId]
  );
  const selectedIndex = useMemo(
    () => selectedOperationIndex({ getId, items, selectedId }),
    [getId, items, selectedId]
  );

  const selectItem = useCallback(
    (item: T) => {
      const nextSelectedId = getId(item);
      pushOperationsUrl(pathForSelectedId(nextSelectedId));
      setSelectedId(nextSelectedId);
    },
    [getId, pathForSelectedId, setSelectedId]
  );

  const selectIndex = useCallback(
    (index: number) => {
      const item = items[index];
      if (item) {
        selectItem(item);
      }
    },
    [items, selectItem]
  );

  return {
    selected,
    selectedIndex,
    selectIndex,
    selectItem,
  };
}
