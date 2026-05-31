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
      <label className="flex h-8 items-center gap-2 border border-white/10 bg-[#090a0d] px-2 font-mono text-slate-500">
        <Search size={13} />
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
          className="w-full bg-transparent text-[11px] text-slate-200 outline-none placeholder:text-slate-700"
          placeholder="trace id / span / correlation / event / function"
          value={query}
        />
        <span className="border border-white/10 px-1 py-0.5 text-[10px] text-slate-600">
          /
        </span>
      </label>
      {open && query.trim() ? (
        <div className="absolute left-0 top-9 z-30 w-[min(620px,calc(100vw-64px))] overflow-hidden border border-white/10 bg-[#090a0d]/98 shadow-[0_28px_90px_rgba(0,0,0,0.48)]">
          {results.length === 0 ? (
            <div className="p-3 font-mono text-[11px] text-slate-600">
              No runtime objects found
            </div>
          ) : (
            results.map((result) => (
              <button
                className="grid w-full grid-cols-[78px_minmax(0,1fr)] gap-3 border-b border-white/10 bg-transparent px-2.5 py-2 text-left font-mono text-slate-100 last:border-b-0 hover:bg-cyan-300/[0.06]"
                key={`${result.kind}:${result.id}`}
                onClick={() => {
                  selectSearchResult(result);
                  setOpen(false);
                  setQuery("");
                }}
              >
                <span className="self-center text-[10px] font-bold uppercase tracking-[0.06em] text-slate-600">
                  {result.kind}
                </span>
                <span>
                  <strong className="block truncate text-[11px] font-semibold">
                    {result.title}
                  </strong>
                  <small className="mt-0.5 block truncate text-[10px] text-slate-600">
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
