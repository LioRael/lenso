import type { ReactNode } from "react";

export const runtimeViewHeaderClassName =
  "flex min-w-0 items-center justify-between gap-3 overflow-hidden border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_42%,transparent)] px-3 py-2";

export const runtimeViewHeaderContentClassName =
  "flex min-w-0 items-center gap-2 overflow-hidden";

export const runtimeViewHeaderLabelClassName =
  "font-sans text-[11px] font-semibold uppercase tracking-[0.08em] text-(--muted)";

export const runtimeViewHeaderSummaryClassName =
  "min-w-0 truncate font-mono text-[11px] text-(--muted)";

export const runtimeViewHeaderMetaClassName =
  "shrink-0 font-mono text-[11px] text-(--muted)";

export function RuntimeViewHeader({
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
    <div className={runtimeViewHeaderClassName}>
      <div className={runtimeViewHeaderContentClassName}>
        <span className={runtimeViewHeaderLabelClassName}>{title}</span>
        {summary ? (
          <span className={runtimeViewHeaderSummaryClassName}>{summary}</span>
        ) : null}
      </div>
      {meta ? (
        <div className={runtimeViewHeaderMetaClassName}>{meta}</div>
      ) : null}
      {children ? (
        <div className={runtimeViewHeaderMetaClassName}>{children}</div>
      ) : null}
    </div>
  );
}
