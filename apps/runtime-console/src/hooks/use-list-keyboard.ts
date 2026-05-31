import { useEffect } from "react";

type UseListKeyboardOptions<T> = {
  items: T[];
  selectedIndex: number;
  setSelectedIndex: (index: number) => void;
  onOpen: (item: T) => void;
  onRetry?: (item: T) => void;
};

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

      if (isTyping || items.length === 0) {
        return;
      }

      if (event.key === "j") {
        event.preventDefault();
        setSelectedIndex(Math.min(selectedIndex + 1, items.length - 1));
      }

      if (event.key === "k") {
        event.preventDefault();
        setSelectedIndex(Math.max(selectedIndex - 1, 0));
      }

      if (event.key === "Enter") {
        event.preventDefault();
        const item = items[selectedIndex];
        if (item) {
          onOpen(item);
        }
      }

      if (event.key.toLowerCase() === "r") {
        const item = items[selectedIndex];
        if (item && onRetry) {
          event.preventDefault();
          onRetry(item);
        }
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [items, onOpen, onRetry, selectedIndex, setSelectedIndex]);
}
