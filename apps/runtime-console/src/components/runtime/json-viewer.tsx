import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";

import { prettyJson } from "../../lib/format";

type JsonViewerProps = {
  title: string;
  value: unknown;
  defaultExpanded?: boolean;
};

export function JsonViewer({
  title,
  value,
  defaultExpanded = false,
}: JsonViewerProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);

  return (
    <section className="overflow-hidden border border-white/10">
      <button
        className="flex w-full items-center gap-2 border-b border-white/10 bg-white/[0.018] px-2.5 py-2 text-left font-mono text-[11px] font-semibold text-slate-500 hover:bg-white/[0.04]"
        onClick={() => setExpanded((current) => !current)}
      >
        {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <span>{title}</span>
        <span className="mono ml-auto text-[11px] text-slate-600">
          {prettyJson(value).split("\n").length} lines
        </span>
      </button>
      {expanded ? (
        <pre className="mono overflow-auto bg-black/25 p-2.5 text-[11px] leading-5 text-slate-300">
          {prettyJson(value)}
        </pre>
      ) : null}
    </section>
  );
}
