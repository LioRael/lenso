import type { PropsWithChildren } from "react";

import { cn } from "../../lib/cn";

function PanelRoot({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <section
      className={cn(
        "overflow-hidden rounded-lg border border-white/10 bg-[#101318]/80 shadow-2xl shadow-black/35",
        className
      )}
    >
      {children}
    </section>
  );
}

function PanelHeader({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-4 border-b border-white/10 p-3.5",
        className
      )}
    >
      {children}
    </div>
  );
}

function PanelTitle({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <h2 className={cn("text-sm font-semibold text-slate-100", className)}>
      {children}
    </h2>
  );
}

function PanelContent({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return <div className={className}>{children}</div>;
}

export const Panel = Object.assign(PanelRoot, {
  Header: PanelHeader,
  Title: PanelTitle,
  Content: PanelContent,
});
