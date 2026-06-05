import { useEffect } from "react";

export type BrowserUrlWriteMode = "push" | "replace";

export function currentBrowserUrl() {
  if (typeof window === "undefined") {
    return "";
  }
  return `${window.location.pathname}${window.location.search}`;
}

export function writeBrowserUrl(
  path: string,
  mode: BrowserUrlWriteMode = "replace"
) {
  if (typeof window === "undefined" || currentBrowserUrl() === path) {
    return;
  }
  if (mode === "push") {
    window.history.pushState(null, "", path);
    return;
  }
  window.history.replaceState(null, "", path);
}

export function useBrowserUrlPopState(
  onPopState: (search: URLSearchParams) => void
) {
  useEffect(() => {
    if (typeof window === "undefined") {
      return undefined;
    }

    const handlePopState = () => {
      onPopState(new URLSearchParams(window.location.search));
    };
    window.addEventListener("popstate", handlePopState);
    return () => window.removeEventListener("popstate", handlePopState);
  }, [onPopState]);
}
