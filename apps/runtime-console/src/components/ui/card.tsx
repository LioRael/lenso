import type { PropsWithChildren } from "react";

import { cn } from "../../lib/cn";

function CardRoot({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <article
      className={cn(
        "rounded-lg border border-[var(--border-subtle)] bg-[color-mix(in_srgb,var(--surface)_88%,transparent)] shadow-2xl shadow-[var(--shadow-strong)]",
        className
      )}
    >
      {children}
    </article>
  );
}

function CardHeader({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <header className={cn("border-b border-[var(--border-subtle)] p-3.5", className)}>
      {children}
    </header>
  );
}

function CardTitle({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <h2 className={cn("text-sm font-semibold text-[var(--foreground)]", className)}>
      {children}
    </h2>
  );
}

function CardDescription({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <p className={cn("mt-1 text-xs leading-5 text-[var(--muted)]", className)}>
      {children}
    </p>
  );
}

function CardContent({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return <div className={cn("p-3.5", className)}>{children}</div>;
}

export const Card = Object.assign(CardRoot, {
  Header: CardHeader,
  Title: CardTitle,
  Description: CardDescription,
  Content: CardContent,
});
