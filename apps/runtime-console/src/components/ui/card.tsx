import type { PropsWithChildren } from "react";

import { cn } from "../../lib/cn";

function CardRoot({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <article
      className={cn(
        "rounded-lg border border-white/10 bg-[#101318]/80 shadow-2xl shadow-black/35",
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
    <header className={cn("border-b border-white/10 p-3.5", className)}>
      {children}
    </header>
  );
}

function CardTitle({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <h2 className={cn("text-sm font-semibold text-slate-100", className)}>
      {children}
    </h2>
  );
}

function CardDescription({
  children,
  className,
}: PropsWithChildren<{ className?: string }>) {
  return (
    <p className={cn("mt-1 text-xs leading-5 text-slate-500", className)}>
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
