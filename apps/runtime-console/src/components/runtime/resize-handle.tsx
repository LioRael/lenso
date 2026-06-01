import { GripVertical } from "lucide-react";

export function ResizeHandle({
  ariaLabel,
  onResize,
  onReset,
}: {
  ariaLabel: string;
  onResize: (deltaX: number) => void;
  onReset?: () => void;
}) {
  return (
    <button
      aria-label={ariaLabel}
      className="group relative z-[1] min-h-0 w-1 cursor-col-resize border-x border-transparent bg-[#1d1d1d] transition hover:bg-[#f3f724]/45 focus-visible:bg-[#f3f724]/45"
      onDoubleClick={onReset}
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
        event.currentTarget.setPointerCapture(event.pointerId);
        const startX = event.clientX;
        let lastDelta = 0;

        const onPointerMove = (moveEvent: PointerEvent) => {
          const delta = moveEvent.clientX - startX;
          onResize(delta - lastDelta);
          lastDelta = delta;
        };

        const onPointerUp = () => {
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
      type="button"
    >
      <span className="absolute inset-y-0 -left-1 -right-1" />
      <GripVertical
        className="pointer-events-none absolute top-1/2 left-1/2 size-3 -translate-x-1/2 -translate-y-1/2 text-transparent transition group-hover:text-black/70 group-focus-visible:text-black/70"
        strokeWidth={2}
      />
    </button>
  );
}
