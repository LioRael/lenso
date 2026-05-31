import { Search } from "lucide-react";
import { useMemo, useState } from "react";

import { useRuntimeConsole } from "./runtime-console-context";

export function RuntimeSearch() {
  const { searchInputRef, searchRuntime, selectSearchResult } =
    useRuntimeConsole();
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);

  const results = useMemo(() => searchRuntime(query), [query, searchRuntime]);

  return (
    <div className="relative">
      <label className="flex h-9 items-center gap-2.5 rounded-lg border border-white/10 bg-white/[0.035] px-3 text-slate-400">
        <Search size={15} />
        <input
          ref={searchInputRef}
          aria-label="Search runtime"
          onBlur={() => window.setTimeout(() => setOpen(false), 120)}
          onChange={(event) => {
            setQuery(event.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              setOpen(false);
            }
          }}
          className="w-full bg-transparent text-[13px] text-slate-100 outline-none placeholder:text-slate-600"
          placeholder="Search id, correlation, event, function..."
          value={query}
        />
        <span className="rounded-md border border-white/10 px-1.5 py-0.5 font-mono text-[11px] text-slate-500">
          /
        </span>
      </label>
      {open && query.trim() ? (
        <div className="absolute left-0 top-11 z-30 w-[min(620px,calc(100vw-64px))] overflow-hidden rounded-xl border border-white/10 bg-[#0c0e12]/98 shadow-[0_28px_90px_rgba(0,0,0,0.48)]">
          {results.length === 0 ? (
            <div className="p-4 text-[13px] text-slate-500">
              No runtime objects found
            </div>
          ) : (
            results.map((result) => (
              <button
                className="grid w-full grid-cols-[84px_minmax(0,1fr)] gap-3 border-b border-white/10 bg-transparent px-3 py-2.5 text-left text-slate-100 last:border-b-0 hover:bg-blue-300/[0.07]"
                key={`${result.kind}:${result.id}`}
                onClick={() => {
                  selectSearchResult(result);
                  setOpen(false);
                  setQuery("");
                }}
              >
                <span className="self-center text-[11px] font-bold uppercase tracking-[0.06em] text-slate-500">
                  {result.kind}
                </span>
                <span>
                  <strong className="block truncate text-[13px] font-semibold">
                    {result.title}
                  </strong>
                  <small className="mono mt-0.5 block truncate text-xs text-slate-500">
                    {result.subtitle}
                  </small>
                </span>
              </button>
            ))
          )}
        </div>
      ) : null}
    </div>
  );
}
