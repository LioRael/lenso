import { useRef, useState } from "react";

export function ResizeHandle({
  ariaLabel,
  axis = "horizontal",
  onResize,
  onReset,
}: {
  ariaLabel: string;
  axis?: "horizontal" | "vertical";
  onResize: (delta: number) => void;
  onReset?: () => void;
}) {
  const [isDragging, setIsDragging] = useState(false);
  const [isFocused, setIsFocused] = useState(false);
  const [isHovered, setIsHovered] = useState(false);
  const handleRef = useRef<HTMLButtonElement | null>(null);
  const suppressHoverRef = useRef(false);
  const isActive = isDragging || isFocused || isHovered;
  const isVertical = axis === "vertical";
  const resizeCursor = isVertical ? "ns-resize" : "col-resize";

  return (
    <button
      aria-label={ariaLabel}
      className={`group relative z-1 bg-transparent outline-hidden ${
        isVertical ? "h-px min-w-0" : "min-h-0 w-px"
      }`}
      ref={handleRef}
      style={{ cursor: resizeCursor }}
      onBlur={() => setIsFocused(false)}
      onDoubleClick={onReset}
      onFocus={() => setIsFocused(true)}
      onKeyDown={(event) => {
        if (event.key === (isVertical ? "ArrowUp" : "ArrowLeft")) {
          event.preventDefault();
          onResize(-16);
        }
        if (event.key === (isVertical ? "ArrowDown" : "ArrowRight")) {
          event.preventDefault();
          onResize(16);
        }
        if (event.key === "Enter") {
          onReset?.();
        }
      }}
      onPointerDown={(event) => {
        setIsDragging(true);
        setIsHovered(true);
        event.currentTarget.setPointerCapture(event.pointerId);
        const start = isVertical ? event.clientY : event.clientX;
        let lastDelta = 0;

        const onPointerMove = (moveEvent: PointerEvent) => {
          const delta =
            (isVertical ? moveEvent.clientY : moveEvent.clientX) - start;
          onResize(delta - lastDelta);
          lastDelta = delta;
        };

        const stopDragging = (upEvent: PointerEvent) => {
          suppressHoverRef.current = true;
          setIsDragging(false);
          setIsHovered(false);
          setIsFocused(false);
          if (handleRef.current?.hasPointerCapture(upEvent.pointerId)) {
            handleRef.current.releasePointerCapture(upEvent.pointerId);
          }
          handleRef.current?.blur();
          window.removeEventListener("pointermove", onPointerMove);
          window.removeEventListener("pointerup", stopDragging);
          window.removeEventListener("pointercancel", stopDragging);
          document.body.style.cursor = "";
          document.body.style.userSelect = "";
        };

        document.body.style.cursor = resizeCursor;
        document.body.style.userSelect = "none";
        window.addEventListener("pointermove", onPointerMove);
        window.addEventListener("pointerup", stopDragging, { once: true });
        window.addEventListener("pointercancel", stopDragging, { once: true });
      }}
      onPointerEnter={() => {
        if (!suppressHoverRef.current) {
          setIsHovered(true);
        }
      }}
      onPointerLeave={() => {
        suppressHoverRef.current = false;
        setIsHovered(false);
      }}
      type="button"
    >
      <span
        className={
          isVertical
            ? "absolute -bottom-1.5 -top-1.5 inset-x-0"
            : "absolute inset-y-0 -left-1.5 -right-1.5"
        }
      />
      <span
        className={`absolute inset-0 transition ${
          isDragging
            ? "bg-[color-mix(in_srgb,var(--accent)_78%,transparent)]"
            : isActive
              ? "bg-[color-mix(in_srgb,var(--accent)_56%,transparent)]"
              : "bg-(--border-subtle)"
        }`}
      />
    </button>
  );
}
