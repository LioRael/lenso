import { AlertTriangle, CheckCircle2 } from "lucide-react";
import type { ReactNode } from "react";

import { cn } from "../lib/cn";

export function OperationsInspectorHeader({
  eyebrow,
  meta,
  title,
}: {
  eyebrow: ReactNode;
  title: ReactNode;
  meta?: ReactNode;
}) {
  return (
    <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2 font-mono">
      <div className="mb-1 text-[9px] font-semibold uppercase tracking-[0.12em] text-(--accent)">
        {eyebrow}
      </div>
      <div className="truncate text-[13px] font-semibold text-(--foreground)">
        {title}
      </div>
      {meta ? (
        <div className="mt-1 flex items-center gap-2 text-[10px] text-(--muted)">
          {meta}
        </div>
      ) : null}
    </header>
  );
}

export function OperationsStatusBanner({
  label,
  summary,
  tone,
}: {
  label: string;
  summary: string;
  tone: "success" | "warning" | "error";
}) {
  return (
    <div
      className={cn(
        "grid grid-cols-[auto_minmax(0,1fr)] gap-2 border-b px-3 py-2 font-mono",
        tone === "success" &&
          "border-[color-mix(in_srgb,var(--success)_32%,transparent)] bg-[color-mix(in_srgb,var(--success)_8%,transparent)]",
        tone === "warning" &&
          "border-[color-mix(in_srgb,#f59e0b_34%,transparent)] bg-[color-mix(in_srgb,#f59e0b_9%,transparent)]",
        tone === "error" &&
          "border-[color-mix(in_srgb,var(--error)_34%,transparent)] bg-[color-mix(in_srgb,var(--error)_9%,transparent)]"
      )}
    >
      {tone === "success" ? (
        <CheckCircle2 className="mt-0.5 text-(--success)" size={14} />
      ) : (
        <AlertTriangle
          className={cn(
            "mt-0.5",
            tone === "warning" ? "text-[#f59e0b]" : "text-(--error)"
          )}
          size={14}
        />
      )}
      <div className="min-w-0">
        <div
          className={cn(
            "text-[11px] font-semibold",
            tone === "success" && "text-(--success)",
            tone === "warning" && "text-[#f59e0b]",
            tone === "error" && "text-(--error)"
          )}
        >
          {label}
        </div>
        <div className="mt-0.5 truncate text-[10px] text-(--secondary)">
          {summary}
        </div>
      </div>
    </div>
  );
}

export function OperationsSectionTitle({ children }: { children: string }) {
  return (
    <div className="border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_52%,transparent)] px-3 py-1.5 font-mono text-[9px] font-semibold uppercase tracking-[0.08em] text-(--muted)">
      {children}
    </div>
  );
}
