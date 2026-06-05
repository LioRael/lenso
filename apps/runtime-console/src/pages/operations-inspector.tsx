import type { ReactNode } from "react";

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
