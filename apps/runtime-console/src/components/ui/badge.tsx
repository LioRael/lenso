import type { PropsWithChildren } from "react";

import { cn } from "../../lib/cn";

export function Badge({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border border-white/10 bg-white/[0.035] px-2.5 py-1 text-[11px] font-semibold text-slate-400",
        className
      )}
    >
      {children}
    </span>
  );
}
