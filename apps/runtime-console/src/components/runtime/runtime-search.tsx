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
      <label className="flex h-7 items-center gap-2 border border-(--border-subtle) bg-(--elevated) px-2 font-mono text-(--muted) shadow-[inset_0_1px_0_rgba(255,255,255,0.04)] transition focus-within:border-(--border)">
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
          className="w-full bg-transparent text-xs text-(--foreground) outline-hidden placeholder:text-(--muted)"
          placeholder="story id / node / correlation / event / function"
          value={query}
        />
        <span className="border border-(--border) px-1 py-0.5 text-[11px] leading-none text-(--muted)">
          /
        </span>
      </label>
      {open && query.trim() ? (
        <div className="absolute left-0 top-9 z-30 w-[min(620px,calc(100vw-64px))] overflow-hidden border border-(--border) bg-(--elevated) shadow-(--elevation-overlay)">
          {results.length === 0 ? (
            <div className="p-3 font-mono text-xs text-(--muted)">
              No runtime objects found
            </div>
          ) : (
            results.map((result) => (
              <button
                className="grid w-full grid-cols-[86px_minmax(0,1fr)] gap-3 border-b border-(--border-subtle) bg-transparent px-2.5 py-2 text-left font-mono text-(--foreground) last:border-b-0 hover:bg-(--hover)"
                key={`${result.kind}:${result.id}`}
                onClick={() => {
                  selectSearchResult(result);
                  setOpen(false);
                  setQuery("");
                }}
                type="button"
              >
                <span className="self-center text-[11px] font-bold uppercase tracking-[0.04em] text-(--muted)">
                  {result.kind}
                </span>
                <span>
                  <strong className="block truncate text-xs font-semibold">
                    {result.title}
                  </strong>
                  <small className="mt-0.5 block truncate text-[11px] text-(--muted)">
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
