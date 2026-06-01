import type { ReactNode } from "react";

export const traceViewHeaderClassName =
  "flex min-w-0 items-center justify-between gap-3 overflow-hidden border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_42%,transparent)] px-3 py-2";

export const traceViewHeaderContentClassName =
  "flex min-w-0 items-center gap-2 overflow-hidden";

export const traceViewHeaderLabelClassName =
  "font-sans text-[11px] font-semibold uppercase tracking-[0.08em] text-(--muted)";

export const traceViewHeaderSummaryClassName =
  "min-w-0 truncate font-mono text-[11px] text-(--muted)";

export const traceViewHeaderMetaClassName =
  "shrink-0 font-mono text-[11px] text-(--muted)";

export function TraceViewHeader({
  children,
  title,
  summary,
  meta,
}: {
  children?: ReactNode;
  title: ReactNode;
  summary?: ReactNode;
  meta?: ReactNode;
}) {
  return (
    <div className={traceViewHeaderClassName}>
      <div className={traceViewHeaderContentClassName}>
        <span className={traceViewHeaderLabelClassName}>{title}</span>
        {summary ? (
          <span className={traceViewHeaderSummaryClassName}>{summary}</span>
        ) : null}
      </div>
      {meta ? <div className={traceViewHeaderMetaClassName}>{meta}</div> : null}
      {children ? (
        <div className={traceViewHeaderMetaClassName}>{children}</div>
      ) : null}
    </div>
  );
}
