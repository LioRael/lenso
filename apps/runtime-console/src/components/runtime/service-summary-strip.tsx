import type { TraceRun } from "../../data/mock-runtime";
import { formatTraceDuration, serviceColor } from "../../lib/trace-style";

export function ServiceSummaryStrip({ trace }: { trace: TraceRun }) {
  const services = Array.from(
    new Set(trace.spans.map((span) => span.service))
  ).map((service) => {
    const spans = trace.spans.filter((span) => span.service === service);
    const durations = spans.map((span) => span.durationMs);
    const duration = durations.reduce(
      (total, spanDuration) => total + spanDuration,
      0
    );
    const errors = spans.filter(
      (span) => span.status === "failed" || span.status === "dead"
    ).length;
    return {
      duration,
      errors,
      p50: percentile(durations, 50),
      p95: percentile(durations, 95),
      p99: percentile(durations, 99),
      service,
      spans: spans.length,
    };
  });

  return (
    <div className="max-h-[142px] min-w-0 overflow-hidden border-t border-[var(--border-subtle)] bg-[var(--surface)]">
      <div className="flex h-7 items-center gap-2 border-b border-[var(--border-subtle)] px-3">
        <span className="font-sans text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--secondary)]">
          Services
        </span>
        <div className="ml-auto flex items-center gap-3 font-mono text-[11px] text-[var(--muted)]">
          <span>
            p50{" "}
            {formatTraceDuration(
              percentile(
                trace.spans.map((span) => span.durationMs),
                50
              )
            )}
          </span>
          <span>
            p95{" "}
            {formatTraceDuration(
              percentile(
                trace.spans.map((span) => span.durationMs),
                95
              )
            )}
          </span>
          <span>
            max{" "}
            {formatTraceDuration(
              Math.max(...trace.spans.map((span) => span.durationMs))
            )}
          </span>
        </div>
      </div>
      <div className="max-h-[114px] overflow-auto">
        {services.map((item) => (
          <div
            className="grid min-w-[700px] grid-cols-[12px_minmax(150px,1fr)_64px_82px_82px_82px_minmax(104px,190px)] items-center gap-2 border-b border-[var(--border-subtle)] px-3 py-1.5 font-mono text-[11px] last:border-b-0"
            key={item.service}
          >
            <div
              className="size-2 rounded-[2px]"
              style={{ backgroundColor: serviceColor(item.service) }}
            />
            <span className="min-w-0 truncate text-xs font-medium text-[var(--foreground)]">
              {item.service}
            </span>
            <span className="text-[var(--muted)]">{item.spans} spans</span>
            <span className="text-[var(--muted)]">
              p50 {formatTraceDuration(item.p50)}
            </span>
            <span className="text-[var(--muted)]">
              p95 {formatTraceDuration(item.p95)}
            </span>
            <span className="text-[var(--muted)]">
              p99 {formatTraceDuration(item.p99)}
            </span>
            <div className="flex min-w-0 items-center gap-2">
              <span
                className={
                  item.errors > 0
                    ? "w-10 text-[#ef4444]"
                    : "w-10 text-[var(--muted)]"
                }
              >
                {item.errors} err
              </span>
              <div className="h-1 flex-1 overflow-hidden rounded-[1px] bg-[var(--elevated)]">
                <div
                  className="h-full rounded-[1px]"
                  style={{
                    backgroundColor: serviceColor(item.service),
                    opacity: 0.7,
                    width: `${Math.max(2, (item.duration / trace.durationMs) * 100)}%`,
                  }}
                />
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function percentile(values: number[], pct: number) {
  if (values.length === 0) {
    return 0;
  }
  const sorted = [...values].sort((left, right) => left - right);
  const index = Math.ceil((pct / 100) * sorted.length) - 1;
  return sorted[Math.max(0, index)] ?? 0;
}
