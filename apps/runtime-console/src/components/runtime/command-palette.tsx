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

import { correlationId, retryTargetFor } from "../../data/mock-runtime";
import { Dialog } from "../ui/dialog";
import { useRuntimeConsole } from "./runtime-console-context";

type CommandItem = {
  id: string;
  title: string;
  subtitle: string;
  action: () => void;
};

type CommandPaletteProps = {
  theme: "dark" | "light";
  onToggleTheme: () => void;
};

export function CommandPalette({ theme, onToggleTheme }: CommandPaletteProps) {
  const navigate = useNavigate();
  const {
    closeCommandPalette,
    commandOpen,
    drawerTarget,
    focusGlobalSearch,
    openRetry,
    openTimeline,
  } = useRuntimeConsole();
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  const commands = useMemo<CommandItem[]>(() => {
    const items: CommandItem[] = [
      {
        action: () => void navigate({ to: "/runtime/stories" }),
        id: "stories",
        subtitle: "Runtime execution stories",
        title: "Go to Stories",
      },
      {
        action: () => void navigate({ to: "/dead-letters" }),
        id: "dead",
        subtitle: "Failure inbox",
        title: "Go to Dead Letters",
      },
      {
        action: () => void navigate({ to: "/queues" }),
        id: "queues",
        subtitle: "Queue pressure",
        title: "Go to Queues",
      },
      {
        action: () => void navigate({ to: "/overview" }),
        id: "overview",
        subtitle: "Runtime health",
        title: "Go to Overview",
      },
      {
        action: onToggleTheme,
        id: "theme-toggle",
        subtitle:
          theme === "dark"
            ? "Use light console theme"
            : "Use dark console theme",
        title:
          theme === "dark" ? "Switch to Light Mode" : "Switch to Dark Mode",
      },
      {
        action: () => {
          openTimeline(correlationId);
          focusGlobalSearch();
        },
        id: "search",
        subtitle: "Correlation search now lives in Stories",
        title: "Search in Stories",
      },
      {
        action: closeCommandPalette,
        id: "copy-correlation",
        subtitle: "Mock copy current timeline correlation",
        title: "Copy correlation ID",
      },
    ];

    if (drawerTarget) {
      const retryTarget = retryTargetFor(drawerTarget);
      items.push({
        action: () => {
          if (retryTarget) {
            openRetry(retryTarget);
          }
        },
        id: "retry-selected",
        subtitle: retryTarget
          ? retryTarget.id
          : "Selected item is not retryable",
        title: "Retry selected item",
      });
    }

    return items;
  }, [
    closeCommandPalette,
    drawerTarget,
    focusGlobalSearch,
    navigate,
    onToggleTheme,
    openRetry,
    openTimeline,
    theme,
  ]);

  const visible = commands.filter((command) =>
    `${command.title} ${command.subtitle}`
      .toLowerCase()
      .includes(query.trim().toLowerCase())
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
