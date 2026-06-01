import type { PropsWithChildren } from "react";

import { cn } from "../../lib/cn";

export function Badge({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 border border-(--border-subtle) bg-white/3.5 px-2.5 py-1 text-[11px] font-semibold text-(--secondary)",
        className
      )}
    >
      {children}
    </span>
  );
}
