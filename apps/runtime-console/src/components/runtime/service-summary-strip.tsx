import { useGSAP } from "@gsap/react";
import gsap from "gsap";
import { ChevronDown } from "lucide-react";
import { useRef, useState } from "react";

import type { TraceRun } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { formatTraceDuration, serviceColor } from "../../lib/trace-style";

gsap.registerPlugin(useGSAP);

export function ServiceSummaryStrip({ trace }: { trace: TraceRun }) {
  const [expanded, setExpanded] = useState(true);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const iconRef = useRef<SVGSVGElement | null>(null);
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

  useGSAP(
    () => {
      const content = contentRef.current;
      const icon = iconRef.current;

      if (!content || !icon) {
        return;
      }

      const reduceMotion = window.matchMedia(
        "(prefers-reduced-motion: reduce)"
      ).matches;
      gsap.killTweensOf([content, icon]);

      if (reduceMotion) {
        gsap.set(content, {
          height: expanded ? "auto" : 0,
          opacity: expanded ? 1 : 0,
        });
        gsap.set(icon, {
          rotate: expanded ? 0 : -90,
        });
        return;
      }

      gsap.to(icon, {
        duration: 0.22,
        ease: "power2.out",
        rotate: expanded ? 0 : -90,
      });

      gsap.to(content, {
        duration: 0.28,
        ease: "power3.out",
        height: expanded ? content.scrollHeight : 0,
        opacity: expanded ? 1 : 0,
        onComplete: () => {
          if (expanded) {
            gsap.set(content, { height: "auto" });
          }
        },
      });
    },
    { dependencies: [expanded, services.length], scope: containerRef }
  );

  return (
    <div
      ref={containerRef}
      className="min-w-0 overflow-hidden border-t border-(--border-subtle) bg-(--surface)"
    >
      <div
        className={cn(
          "flex h-7 min-w-0 items-center gap-2 px-3",
          expanded && "border-b border-(--border-subtle)"
        )}
      >
        <button
          aria-expanded={expanded}
          aria-label={expanded ? "Collapse services" : "Expand services"}
          className="flex min-w-0 items-center gap-1.5 text-left transition hover:text-(--foreground)"
          onClick={() => setExpanded((current) => !current)}
          type="button"
        >
          <ChevronDown
            ref={iconRef}
            className="shrink-0 text-(--muted)"
            size={13}
          />
          <span className="font-sans text-[11px] font-semibold uppercase tracking-[0.08em] text-(--secondary)">
            Services
          </span>
          <span className="rounded-[2px] border border-(--border-subtle) bg-(--elevated) px-1 font-mono text-[10px] text-(--muted)">
            {services.length}
          </span>
        </button>
        <div className="ml-auto flex min-w-0 items-center gap-3 overflow-hidden font-mono text-[11px] text-(--muted)">
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
      <div
        ref={contentRef}
        className="max-h-28.5 overflow-hidden"
        style={{ height: "auto" }}
      >
        <div className="max-h-28.5 overflow-auto">
          {services.map((item) => (
            <div
              className="grid min-w-175 grid-cols-[12px_minmax(150px,1fr)_64px_82px_82px_82px_minmax(104px,190px)] items-center gap-2 border-b border-(--border-subtle) px-3 py-1.5 font-mono text-[11px] last:border-b-0"
              key={item.service}
            >
              <div
                className="size-2 rounded-xs"
                style={{ backgroundColor: serviceColor(item.service) }}
              />
              <span className="min-w-0 truncate text-xs font-medium text-(--foreground)">
                {item.service}
              </span>
              <span className="text-(--muted)">{item.spans} spans</span>
              <span className="text-(--muted)">
                p50 {formatTraceDuration(item.p50)}
              </span>
              <span className="text-(--muted)">
                p95 {formatTraceDuration(item.p95)}
              </span>
              <span className="text-(--muted)">
                p99 {formatTraceDuration(item.p99)}
              </span>
              <div className="flex min-w-0 items-center gap-2">
                <span
                  className={
                    item.errors > 0
                      ? "w-10 text-[#ef4444]"
                      : "w-10 text-(--muted)"
                  }
                >
                  {item.errors} err
                </span>
                <div className="h-1 flex-1 overflow-hidden rounded-[1px] bg-(--elevated)">
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
