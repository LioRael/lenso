import { ScrollArea } from "@base-ui/react/scroll-area";
import type { ReactNode, WheelEvent } from "react";

import { cn } from "../../lib/cn";

export function HorizontalScrollArea({
  children,
  className,
  contentClassName,
  viewportClassName,
}: {
  children: ReactNode;
  className?: string;
  contentClassName?: string;
  viewportClassName?: string;
}) {
  return (
    <ScrollArea.Root
      className={cn("relative min-w-0", className)}
      overflowEdgeThreshold={1}
    >
      <ScrollArea.Viewport
        className={cn(
          "scrollbar-none min-w-0 overflow-x-auto overflow-y-hidden",
          viewportClassName
        )}
        onWheel={handleWheel}
      >
        <ScrollArea.Content
          className={cn("w-max min-w-full", contentClassName)}
        >
          {children}
        </ScrollArea.Content>
      </ScrollArea.Viewport>
    </ScrollArea.Root>
  );
}

export function HorizontalTabScroll({ children }: { children: ReactNode }) {
  return (
    <HorizontalScrollArea
      className="h-8"
      contentClassName="h-full"
      viewportClassName="h-full"
    >
      {children}
    </HorizontalScrollArea>
  );
}

function handleWheel(event: WheelEvent<HTMLDivElement>) {
  const viewport = event.currentTarget;
  const maxScrollLeft = viewport.scrollWidth - viewport.clientWidth;

  if (maxScrollLeft <= 0) {
    return;
  }

  const delta =
    Math.abs(event.deltaX) > Math.abs(event.deltaY)
      ? event.deltaX
      : event.deltaY;

  if (delta === 0) {
    return;
  }

  const nextScrollLeft = Math.max(
    0,
    Math.min(maxScrollLeft, viewport.scrollLeft + delta)
  );

  if (nextScrollLeft === viewport.scrollLeft) {
    return;
  }

  event.preventDefault();
  viewport.scrollLeft = nextScrollLeft;
}
