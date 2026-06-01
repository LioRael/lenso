import type { ButtonHTMLAttributes, PropsWithChildren } from "react";

import { cn } from "../../lib/cn";

type ButtonVariant = "default" | "ghost" | "danger";

type ButtonProps = PropsWithChildren<
  ButtonHTMLAttributes<HTMLButtonElement> & {
    variant?: ButtonVariant;
  }
>;

export function Button({
  children,
  className,
  variant = "default",
  ...props
}: ButtonProps) {
  return (
    <button
      className={cn(
        "inline-flex min-h-7 items-center justify-center gap-1.5 border px-2.5 font-mono text-[11px] font-semibold transition focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-300 disabled:cursor-not-allowed disabled:opacity-45",
        variant === "default" &&
          "border-[var(--border-subtle)] bg-[var(--elevated)] text-[var(--foreground)] shadow-[inset_0_1px_0_var(--panel-gloss)] hover:border-[var(--border)] hover:bg-[var(--hover)]",
        variant === "ghost" &&
          "border-transparent bg-transparent text-[var(--secondary)] hover:bg-[var(--hover)] hover:text-[var(--foreground)]",
        variant === "danger" &&
          "border-[color-mix(in_srgb,var(--error)_35%,transparent)] bg-[color-mix(in_srgb,var(--error)_10%,transparent)] text-[var(--foreground)] hover:bg-[color-mix(in_srgb,var(--error)_15%,transparent)]",
        className
      )}
      {...props}
    >
      {children}
    </button>
  );
}
