import { Search } from "lucide-react";
import type { PropsWithChildren } from "react";

import { cn } from "../lib/cn";

export function OperationsFilterBar({ children }: PropsWithChildren) {
  return (
    <div className="flex h-9 items-center gap-2 border-b border-(--border-subtle) bg-(--background) px-3">
      {children}
    </div>
  );
}

export function OperationsSearchInput({
  ariaLabel,
  className,
  onChange,
  placeholder,
  value,
}: {
  ariaLabel: string;
  className?: string;
  onChange: (value: string) => void;
  placeholder: string;
  value: string;
}) {
  return (
    <label
      className={cn(
        "ml-auto flex h-6 items-center gap-2 border border-(--border-subtle) bg-(--elevated) px-2 font-mono text-(--muted)",
        className ?? "w-[min(360px,45vw)]"
      )}
    >
      <Search size={12} />
      <input
        aria-label={ariaLabel}
        className="w-full bg-transparent text-[10px] text-(--foreground) outline-hidden placeholder:text-(--muted)"
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        value={value}
      />
    </label>
  );
}
