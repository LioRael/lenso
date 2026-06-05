import { afterEach, describe, expect, test, vi } from "vitest";

import { currentBrowserUrl, writeBrowserUrl } from "./use-browser-url-state";

describe("browser url state helpers", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  test("reads the current browser path and search", () => {
    stubWindow("/runtime/stories", "?story=corr_1");

    expect(currentBrowserUrl()).toBe("/runtime/stories?story=corr_1");
  });

  test("does not write history when the path is unchanged", () => {
    const history = stubWindow("/runtime/stories", "?story=corr_1");

    writeBrowserUrl("/runtime/stories?story=corr_1", "push");

    expect(history.pushState).not.toHaveBeenCalled();
    expect(history.replaceState).not.toHaveBeenCalled();
  });

  test("pushes or replaces changed urls", () => {
    const history = stubWindow("/runtime/stories", "?story=corr_1");

    writeBrowserUrl("/runtime/stories?story=corr_2", "push");
    writeBrowserUrl("/runtime/stories?story=corr_3", "replace");

    expect(history.pushState).toHaveBeenCalledWith(
      null,
      "",
      "/runtime/stories?story=corr_2"
    );
    expect(history.replaceState).toHaveBeenCalledWith(
      null,
      "",
      "/runtime/stories?story=corr_3"
    );
  });
});

function stubWindow(pathname: string, search: string) {
  const history = {
    pushState: vi.fn(),
    replaceState: vi.fn(),
  };
  vi.stubGlobal("window", {
    addEventListener: vi.fn(),
    history,
    location: { pathname, search },
    removeEventListener: vi.fn(),
  });
  return history;
}
