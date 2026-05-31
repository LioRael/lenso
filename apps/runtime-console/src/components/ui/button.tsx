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
          "border-white/10 bg-white/[0.04] text-slate-100 hover:bg-white/[0.07]",
        variant === "ghost" &&
          "border-white/10 bg-transparent text-slate-300 hover:bg-white/[0.06] hover:text-white",
        variant === "danger" &&
          "border-rose-400/35 bg-rose-400/10 text-rose-100 hover:bg-rose-400/15",
        className
      )}
      {...props}
    >
      {children}
    </button>
  );
}
