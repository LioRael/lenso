import type { PropsWithChildren, ReactNode } from "react";

import { cn } from "../../lib/cn";

function EmptyStateRoot({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <div
      className={cn(
        "grid place-items-center gap-2 p-12 text-center text-slate-500",
        className
      )}
    >
      {children}
    </div>
  );
}

function EmptyStateIcon({ children }: { children: ReactNode }) {
  return <div className="text-slate-500">{children}</div>;
}

function EmptyStateTitle({ children }: PropsWithChildren) {
  return <h2 className="text-base font-semibold text-slate-100">{children}</h2>;
}

function EmptyStateDescription({ children }: PropsWithChildren) {
  return (
    <p className="max-w-md text-sm leading-6 text-slate-500">{children}</p>
  );
}

export const EmptyState = Object.assign(EmptyStateRoot, {
  Icon: EmptyStateIcon,
  Title: EmptyStateTitle,
  Description: EmptyStateDescription,
});
