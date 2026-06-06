import { useNavigate } from "@tanstack/react-router";
import {
  Copy,
  CornerDownLeft,
  GitBranch,
  Moon,
  RotateCcw,
  Search,
  Sun,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { useConsoleNavigation } from "../../app/console-module-metadata";
import { runtimeStories } from "../../data/mock-runtime";
import { queryDataWithMockFallback } from "../../hooks/runtime-query-data";
import { currentBrowserUrl } from "../../hooks/use-browser-url-state";
import { useRuntimeStories } from "../../hooks/use-runtime-queries";
import { isApiMode } from "../../lib/http-client";
import { Dialog } from "../ui/dialog";
import {
  buildStoryCommandItems,
  type CommandItem,
} from "./command-palette-model";
import { useRuntimeConsole } from "./runtime-console-context";

type CommandPaletteProps = {
  theme: "dark" | "light";
  onToggleTheme: () => void;
};

export function CommandPalette({ theme, onToggleTheme }: CommandPaletteProps) {
  const navigate = useNavigate();
  const { closeCommandPalette, commandOpen, focusGlobalSearch, openStory } =
    useRuntimeConsole();
  const consoleNavigation = useConsoleNavigation();
  const storiesQuery = useRuntimeStories();
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  const commands = useMemo<CommandItem[]>(() => {
    const stories = queryDataWithMockFallback({
      apiMode: isApiMode(),
      data: storiesQuery.data,
      fallback: runtimeStories,
      isError: storiesQuery.isError,
    });
    const storyItems = buildStoryCommandItems({
      onOpenStory: openStory,
      stories,
    });
    const consoleItems: CommandItem[] = consoleNavigation.map((item) => {
      const workspaceLabel = item.navigation?.workspace.label ?? "System";

      return {
        action: () => void navigate({ to: item.path }),
        id: `console:${item.moduleId}:${item.path}`,
        searchText:
          `go to ${item.label} ${workspaceLabel} ${item.moduleId} ${item.path}`.toLowerCase(),
        subtitle: `${workspaceLabel} / ${item.moduleId}`,
        title: `Go to ${item.label}`,
      };
    });
    const items: CommandItem[] = [
      ...consoleItems,
      {
        action: () => void navigate({ to: "/operations/dead-letters" }),
        id: "dead",
        searchText: "go to dead letters failure inbox",
        subtitle: "Failure inbox",
        title: "Go to Dead Letters",
      },
      {
        action: () => void navigate({ to: "/operations/functions" }),
        id: "functions",
        searchText: "go to functions runtime function runs module queue",
        subtitle: "Runtime function runs",
        title: "Go to Functions",
      },
      {
        action: () => void navigate({ to: "/operations/remote-calls" }),
        id: "remote-calls",
        searchText: "go to remote calls proxy module remote operations",
        subtitle: "Remote proxy calls",
        title: "Go to Remote Calls",
      },
      {
        action: () => void navigate({ to: "/operations/admin-actions" }),
        id: "admin-actions",
        searchText: "go to admin actions module action operations",
        subtitle: "Admin action invocations",
        title: "Go to Admin Actions",
      },
      {
        action: () => void navigate({ to: "/operations/queues" }),
        id: "queues",
        searchText: "go to queues queue pressure",
        subtitle: "Queue pressure",
        title: "Go to Queues",
      },
      {
        action: () => void navigate({ to: "/overview" }),
        id: "overview",
        searchText: "go to overview runtime health",
        subtitle: "Runtime health",
        title: "Go to Overview",
      },
      {
        action: onToggleTheme,
        id: "theme-toggle",
        searchText: "switch theme light dark mode console",
        subtitle:
          theme === "dark"
            ? "Use light console theme"
            : "Use dark console theme",
        title:
          theme === "dark" ? "Switch to Light Mode" : "Switch to Dark Mode",
      },
      {
        action: () => {
          focusGlobalSearch();
        },
        id: "search",
        searchText: "search in stories correlation story execution",
        subtitle: "Correlation search now lives in Stories",
        title: "Search in Stories",
      },
      {
        action: copyCurrentLink,
        id: "copy-current-link",
        searchText: "copy current link url permalink deep link",
        subtitle: "Copy the current console URL",
        title: "Copy Current Link",
      },
      ...storyItems,
    ];

    return items;
  }, [
    focusGlobalSearch,
    consoleNavigation,
    navigate,
    onToggleTheme,
    openStory,
    storiesQuery.data,
    storiesQuery.isError,
    theme,
  ]);

  const visible = commands.filter((command) =>
    command.searchText.includes(query.trim().toLowerCase())
  );

  useEffect(() => {
    if (commandOpen) {
      setQuery("");
      setActiveIndex(0);
      window.setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [commandOpen]);

  const runCommand = (command: CommandItem | undefined) => {
    if (!command) {
      return;
    }
    command.action();
    closeCommandPalette();
  };

  return (
    <Dialog
      onOpenChange={(open) => !open && closeCommandPalette()}
      open={commandOpen}
    >
      <Dialog.Portal>
        <Dialog.Backdrop className="z-60" />
        <Dialog.Popup
          aria-label="Command palette"
          className="z-70 w-[min(640px,calc(100vw-28px))]"
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              closeCommandPalette();
            }
            if (event.key === "ArrowDown") {
              event.preventDefault();
              if (visible.length > 0) {
                setActiveIndex((index) =>
                  Math.min(index + 1, visible.length - 1)
                );
              }
            }
            if (event.key === "ArrowUp") {
              event.preventDefault();
              if (visible.length > 0) {
                setActiveIndex((index) => Math.max(index - 1, 0));
              }
            }
            if (event.key === "Enter") {
              runCommand(visible[activeIndex]);
            }
          }}
        >
          <div className="flex items-center gap-2.5 border-b border-(--border-subtle) px-3 py-2.5 text-(--secondary)">
            <Search size={16} />
            <input
              aria-label="Command search"
              className="w-full bg-transparent font-mono text-xs text-(--foreground) outline-hidden placeholder:text-(--muted-deep)"
              onChange={(event) => {
                setQuery(event.target.value);
                setActiveIndex(0);
              }}
              placeholder="Type a command..."
              ref={inputRef}
              value={query}
            />
          </div>
          <div className="max-h-105 overflow-auto p-1">
            {visible.map((command, index) => (
              <button
                className={`grid w-full grid-cols-[24px_minmax(0,1fr)_auto] items-center gap-2 border border-transparent p-2 text-left font-mono text-(--foreground) ${
                  index === activeIndex
                    ? "border-[color-mix(in_srgb,var(--accent)_32%,transparent)] bg-(--accent-soft)"
                    : "hover:bg-(--hover)"
                }`}
                key={command.id}
                onClick={() => runCommand(command)}
                type="button"
              >
                <CommandIcon id={command.id} />
                <span className="min-w-0">
                  <strong className="block truncate text-[11px] font-semibold">
                    {command.title}
                  </strong>
                  <small className="mt-0.5 block truncate text-[10px] text-(--muted)">
                    {command.subtitle}
                  </small>
                </span>
                <CornerDownLeft className="text-(--muted)" size={14} />
              </button>
            ))}
          </div>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog>
  );
}

function copyCurrentLink() {
  if (typeof window === "undefined") {
    return;
  }
  const url = new URL(currentBrowserUrl(), window.location.origin).toString();
  void window.navigator.clipboard?.writeText(url);
}

function CommandIcon({ id }: { id: string }) {
  if (id === "theme-toggle") {
    return <ThemeCommandIcon />;
  }
  if (id.includes("retry")) {
    return <RotateCcw size={15} />;
  }
  if (id.includes("copy")) {
    return <Copy size={15} />;
  }
  return <GitBranch size={15} />;
}

function ThemeCommandIcon() {
  return (
    <span className="grid size-3.75 place-items-center">
      <Sun className="hidden [[data-theme=dark]_&]:block" size={15} />
      <Moon className="block [[data-theme=dark]_&]:hidden" size={15} />
    </span>
  );
}
