import type { PropsWithChildren } from "react";

import { cn } from "../lib/cn";

export function OperationsTableHeader({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <div
      className={cn(
        "grid h-7 items-center border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_52%,transparent)] px-3 font-mono text-[9px] uppercase tracking-[0.08em] text-(--muted)",
        className
      )}
    >
      {children}
    </div>
  );
}

export function OperationsSelectableRow({
  children,
  className,
  isSelected,
  onClick,
}: PropsWithChildren<{
  className?: string;
  isSelected: boolean;
  onClick: () => void;
}>) {
  return (
    <button
      className={cn(
        "grid w-full items-center border-b border-(--border-subtle) px-3 text-left font-mono text-[11px]",
        isSelected
          ? "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
          : "hover:bg-(--elevated)",
        className
      )}
      onClick={onClick}
      type="button"
    >
      {children}
    </button>
  );
}

export function OperationsAggregateRow({
  children,
  className,
  onClick,
}: PropsWithChildren<{
  className?: string;
  onClick: () => void;
}>) {
  return (
    <button
      className={cn(
        "grid h-8 w-full items-center border-b border-(--border-subtle) px-3 text-left font-mono text-[10px] hover:bg-(--elevated)",
        className
      )}
      onClick={onClick}
      type="button"
    >
      {children}
    </button>
  );
}
