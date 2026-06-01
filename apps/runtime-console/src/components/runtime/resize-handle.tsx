import { useState } from "react";

export function ResizeHandle({
  ariaLabel,
  onResize,
  onReset,
}: {
  ariaLabel: string;
  onResize: (deltaX: number) => void;
  onReset?: () => void;
}) {
  const [isDragging, setIsDragging] = useState(false);
  const [isFocused, setIsFocused] = useState(false);
  const [isHovered, setIsHovered] = useState(false);
  const isActive = isDragging || isFocused || isHovered;

  return (
    <button
      aria-label={ariaLabel}
      className="group relative z-[1] min-h-0 w-px cursor-col-resize bg-transparent outline-none"
      onBlur={() => setIsFocused(false)}
      onDoubleClick={onReset}
      onFocus={() => setIsFocused(true)}
      onKeyDown={(event) => {
        if (event.key === "ArrowLeft") {
          event.preventDefault();
          onResize(-16);
        }
        if (event.key === "ArrowRight") {
          event.preventDefault();
          onResize(16);
        }
        if (event.key === "Enter") {
          onReset?.();
        }
      }}
      onPointerDown={(event) => {
        setIsDragging(true);
        event.currentTarget.setPointerCapture(event.pointerId);
        const startX = event.clientX;
        let lastDelta = 0;

        const onPointerMove = (moveEvent: PointerEvent) => {
          const delta = moveEvent.clientX - startX;
          onResize(delta - lastDelta);
          lastDelta = delta;
        };

        const onPointerUp = () => {
          setIsDragging(false);
          window.removeEventListener("pointermove", onPointerMove);
          window.removeEventListener("pointerup", onPointerUp);
          document.body.style.cursor = "";
          document.body.style.userSelect = "";
        };

        document.body.style.cursor = "col-resize";
        document.body.style.userSelect = "none";
        window.addEventListener("pointermove", onPointerMove);
        window.addEventListener("pointerup", onPointerUp, { once: true });
      }}
      onPointerEnter={() => setIsHovered(true)}
      onPointerLeave={() => setIsHovered(false)}
      type="button"
    >
      <span className="absolute inset-y-0 -left-1.5 -right-1.5" />
      <span
        className={`absolute inset-0 transition ${
          isDragging
            ? "bg-[color-mix(in_srgb,var(--accent)_78%,transparent)]"
            : isActive
              ? "bg-[color-mix(in_srgb,var(--accent)_56%,transparent)]"
              : "bg-[var(--border-subtle)]"
        }`}
      />
    </button>
  );
}
