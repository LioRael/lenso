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
          "border-[#1d1d1d] bg-[#111111] text-[#f4f4f4] hover:bg-[#1a1a1a]",
        variant === "ghost" &&
          "border-transparent bg-transparent text-[#9ca3af] hover:bg-[#1a1a1a] hover:text-[#f4f4f4]",
        variant === "danger" &&
          "border-[#ef4444]/35 bg-[#ef4444]/10 text-[#f4f4f4] hover:bg-[#ef4444]/15",
        className
      )}
      {...props}
    >
      {children}
    </button>
  );
}
