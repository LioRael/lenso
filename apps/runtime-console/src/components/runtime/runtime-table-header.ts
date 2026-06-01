export const runtimeTableHeaderBaseClassName =
  "grid min-w-0 gap-4 border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_52%,transparent)] px-3 py-2 font-sans text-[11px] font-semibold uppercase tracking-[0.06em] text-(--muted)";

export const runtimeTimelineTableHeaderClassName = `${runtimeTableHeaderBaseClassName} grid-cols-[minmax(180px,260px)_minmax(0,1fr)] max-md:grid-cols-1`;

export const runtimeWaterfallTableHeaderClassName = `${runtimeTableHeaderBaseClassName} grid-cols-[minmax(260px,340px)_minmax(0,1fr)]`;
