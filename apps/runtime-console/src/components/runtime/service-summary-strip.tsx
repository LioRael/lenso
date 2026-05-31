import type { TraceRun } from "../../data/mock-runtime";

export function ServiceSummaryStrip({ trace }: { trace: TraceRun }) {
  const services = Array.from(
    new Set(trace.spans.map((span) => span.service))
  ).map((service) => {
    const spans = trace.spans.filter((span) => span.service === service);
    const duration = spans.reduce((total, span) => total + span.durationMs, 0);
    const errors = spans.filter(
      (span) => span.status === "failed" || span.status === "dead"
    ).length;
    return { duration, errors, service, spans: spans.length };
  });

  return (
    <div className="grid grid-cols-[repeat(auto-fit,minmax(132px,1fr))] border-t border-white/10 bg-[#07080a]">
      {services.map((item) => (
        <div
          className="border-r border-white/10 px-2 py-1.5 font-mono text-[10px]"
          key={item.service}
        >
          <div className="truncate text-slate-300">{item.service}</div>
          <div className="mt-1 flex gap-3 text-slate-600">
            <span>{item.spans} spans</span>
            <span>{item.duration}ms</span>
            <span className={item.errors > 0 ? "text-rose-300" : ""}>
              {item.errors} errors
            </span>
          </div>
        </div>
      ))}
    </div>
  );
}
