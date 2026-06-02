import type { PropsWithChildren } from "react";

import { cn } from "../../lib/cn";

function PanelRoot({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <section
      className={cn(
        "overflow-hidden rounded-lg border border-(--border-subtle) bg-[color-mix(in_srgb,var(--surface)_88%,transparent)] shadow-(--elevation-card)",
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
        "flex items-center justify-between gap-4 border-b border-(--border-subtle) p-3.5",
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
    <h2 className={cn("text-sm font-semibold text-(--foreground)", className)}>
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
