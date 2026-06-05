import { useEffect } from "react";

type UseListKeyboardOptions<T> = {
  items: T[];
  selectedIndex: number;
  setSelectedIndex: (index: number) => void;
  onOpen: (item: T) => void;
  onRetry?: (item: T) => void;
};

export type ListKeyboardAction<T> =
  | { kind: "open"; item: T }
  | { index: number; kind: "select" }
  | { kind: "retry"; item: T };

export function listKeyboardAction<T>({
  hasModifier,
  isTyping,
  items,
  key,
  retryEnabled,
  selectedIndex,
}: {
  items: T[];
  selectedIndex: number;
  key: string;
  isTyping: boolean;
  hasModifier: boolean;
  retryEnabled: boolean;
}): ListKeyboardAction<T> | null {
  if (isTyping || hasModifier || items.length === 0) {
    return null;
  }

  if (key === "j") {
    return {
      index: Math.min(selectedIndex + 1, items.length - 1),
      kind: "select",
    };
  }

  if (key === "k") {
    return {
      index: Math.max(selectedIndex - 1, 0),
      kind: "select",
    };
  }

  const item = items[selectedIndex];
  if (!item) {
    return null;
  }

  if (key === "Enter") {
    return { item, kind: "open" };
  }

  if (key.toLowerCase() === "r" && retryEnabled) {
    return { item, kind: "retry" };
  }

  return null;
}

export function useListKeyboard<T>({
  items,
  selectedIndex,
  setSelectedIndex,
  onOpen,
  onRetry,
}: UseListKeyboardOptions<T>) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const isTyping =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        target?.tagName === "SELECT" ||
        target?.isContentEditable;
      const hasModifier = event.metaKey || event.ctrlKey || event.altKey;

      const action = listKeyboardAction({
        hasModifier,
        isTyping: Boolean(isTyping),
        items,
        key: event.key,
        retryEnabled: Boolean(onRetry),
        selectedIndex,
      });

      if (!action) {
        return;
      }

      if (action.kind === "select") {
        event.preventDefault();
        setSelectedIndex(action.index);
        return;
      }

      if (action.kind === "open") {
        event.preventDefault();
        onOpen(action.item);
        return;
      }

      if (action.kind === "retry" && onRetry) {
        event.preventDefault();
        onRetry(action.item);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [items, onOpen, onRetry, selectedIndex, setSelectedIndex]);
}
