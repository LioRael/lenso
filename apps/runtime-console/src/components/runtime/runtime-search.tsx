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
      <label className="flex h-7 items-center gap-2 rounded-[6px] border border-[#1d1d1d] bg-[#111111] px-2 font-mono text-[#5b5b5b]">
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
          className="w-full bg-transparent text-[11px] text-[#f4f4f4] outline-none placeholder:text-[#5b5b5b]"
          placeholder="trace id / span / correlation / event / function"
          value={query}
        />
        <span className="border border-[#2d2d2d] px-1 py-0.5 text-[10px] text-[#5b5b5b]">
          /
        </span>
      </label>
      {open && query.trim() ? (
        <div className="absolute left-0 top-9 z-30 w-[min(620px,calc(100vw-64px))] overflow-hidden rounded-[10px] border border-[#2d2d2d] bg-[#111111] shadow-[0_28px_90px_rgba(0,0,0,0.62)]">
          {results.length === 0 ? (
            <div className="p-3 font-mono text-[11px] text-[#5b5b5b]">
              No runtime objects found
            </div>
          ) : (
            results.map((result) => (
              <button
                className="grid w-full grid-cols-[78px_minmax(0,1fr)] gap-3 border-b border-[#1d1d1d] bg-transparent px-2.5 py-2 text-left font-mono text-[#f4f4f4] last:border-b-0 hover:bg-[#1a1a1a]"
                key={`${result.kind}:${result.id}`}
                onClick={() => {
                  selectSearchResult(result);
                  setOpen(false);
                  setQuery("");
                }}
              >
                <span className="self-center text-[10px] font-bold uppercase tracking-[0.06em] text-[#5b5b5b]">
                  {result.kind}
                </span>
                <span>
                  <strong className="block truncate text-[11px] font-semibold">
                    {result.title}
                  </strong>
                  <small className="mt-0.5 block truncate text-[10px] text-[#5b5b5b]">
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
