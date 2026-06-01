import { Inbox } from "lucide-react";

import { queueHealth } from "../data/mock-runtime";

export function QueuesPage() {
  const totals = queueHealth.reduce(
    (acc, queue) => ({
      dead: acc.dead + queue.dead,
      failed: acc.failed + queue.failed,
      pending: acc.pending + queue.pending,
      running: acc.running + queue.running,
    }),
    { dead: 0, failed: 0, pending: 0, running: 0 }
  );

  return (
    <section className="grid h-full min-h-0 grid-rows-[auto_auto_minmax(0,1fr)] overflow-hidden bg-black text-[#f4f4f4]">
      <header className="border-b border-[#1d1d1d] bg-[#0a0a0a] px-3 py-2">
        <div className="flex items-center gap-2">
          <Inbox className="text-[#f3f724]" size={14} />
          <h1 className="font-mono text-[13px] font-semibold">Queues</h1>
          <span className="ml-auto font-mono text-[10px] text-[#5b5b5b]">
            pressure lanes / mock
          </span>
        </div>
      </header>

      <div className="grid border-b border-[#1d1d1d] bg-[#0a0a0a] md:grid-cols-4">
        {[
          ["pending", totals.pending],
          ["running", totals.running],
          ["failed", totals.failed],
          ["dead", totals.dead],
        ].map(([label, value]) => (
          <div
            className="grid grid-cols-[minmax(0,1fr)_auto] border-r border-[#1d1d1d] px-3 py-2 font-mono text-[10px] last:border-r-0"
            key={label}
          >
            <span className="text-[#5b5b5b]">{label}</span>
            <span className="text-[13px] font-semibold text-[#f4f4f4]">
              {value}
            </span>
          </div>
        ))}
      </div>

      <div className="min-h-0 overflow-auto">
        <div className="grid h-7 grid-cols-[minmax(180px,1fr)_72px_72px_72px_72px_92px_minmax(120px,240px)] items-center gap-2 border-b border-[#1d1d1d] bg-[#111111]/50 px-3 font-mono text-[9px] uppercase tracking-[0.08em] text-[#5b5b5b]">
          <span>queue</span>
          <span>pending</span>
          <span>running</span>
          <span>failed</span>
          <span>dead</span>
          <span>oldest</span>
          <span>pressure</span>
        </div>
        {queueHealth.map((queue) => {
          const total =
            queue.pending + queue.running + queue.failed + queue.dead;
          return (
            <div
              className="grid min-h-11 grid-cols-[minmax(180px,1fr)_72px_72px_72px_72px_92px_minmax(120px,240px)] items-center gap-2 border-b border-[#1d1d1d] px-3 font-mono text-[11px]"
              key={queue.name}
            >
              <span className="truncate text-[#f4f4f4]">{queue.name}</span>
              <span className="text-[#9ca3af]">{queue.pending}</span>
              <span className="text-[#9ca3af]">{queue.running}</span>
              <span
                className={
                  queue.failed > 0 ? "text-[#ef4444]" : "text-[#5b5b5b]"
                }
              >
                {queue.failed}
              </span>
              <span
                className={queue.dead > 0 ? "text-[#ef4444]" : "text-[#5b5b5b]"}
              >
                {queue.dead}
              </span>
              <span className="text-[#5b5b5b]">{queue.oldest}</span>
              <span className="flex min-w-0 items-center gap-2">
                <span className="h-1 flex-1 overflow-hidden rounded-[1px] bg-[#111111]">
                  <span
                    className="block h-full rounded-[1px] bg-[#f3f724]"
                    style={{
                      width: `${Math.min(100, Math.max(4, total * 5))}%`,
                    }}
                  />
                </span>
                <span className="w-8 text-right text-[#5b5b5b]">{total}</span>
              </span>
            </div>
          );
        })}
      </div>
    </section>
  );
}
