import { useGSAP } from "@gsap/react";
import { Link } from "@tanstack/react-router";
import gsap from "gsap";
import {
  Activity,
  Boxes,
  Command,
  Database,
  Moon,
  Network,
  PanelLeftClose,
  PanelLeftOpen,
  Settings,
  Sun,
  Workflow,
} from "lucide-react";
import { useCallback, useEffect, useRef } from "react";
import type { ComponentType, CSSProperties, PropsWithChildren } from "react";

import { usePersistedLayout } from "../../hooks/use-persisted-layout";
import { runtimeConsoleDataSource } from "../../lib/http-client";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { CommandPalette } from "./command-palette";
import { DetailDrawer } from "./detail-drawer";
import { RetryDialog } from "./retry-dialog";
import { useRuntimeConsole } from "./runtime-console-context";
import { RuntimeSearch } from "./runtime-search";

gsap.registerPlugin(useGSAP);

const primaryNavItems = [
  { to: "/overview", label: "Overview", icon: Activity },
  { to: "/runtime/stories", label: "Stories", icon: Workflow },
  { to: "/operations", label: "Operations", icon: Network },
  { to: "/modules", label: "Modules", icon: Boxes },
  { to: "/data", label: "Data", icon: Database },
] as const;

const configNavItem = {
  to: "/config",
  label: "Configuration",
  icon: Settings,
} as const;

export function RuntimeConsoleShell({ children }: PropsWithChildren) {
  const shellRef = useRef<HTMLDivElement>(null);
  const { closeDrawer, drawerTarget, focusGlobalSearch, openCommandPalette } =
    useRuntimeConsole();
  const [sidebarCollapsed, setSidebarCollapsed] = usePersistedLayout(
    "runtime-console:sidebar-collapsed",
    false
  );
  const [theme, setTheme] = usePersistedLayout<"dark" | "light">(
    "runtime-console:theme",
    "dark"
  );
  const initialCollapseRef = useRef(sidebarCollapsed ? 1 : 0);
  const animateSidebarRef = useRef(false);
  const previousSidebarCollapsedRef = useRef(sidebarCollapsed);

  const toggleSidebar = useCallback(() => {
    animateSidebarRef.current = true;
    setSidebarCollapsed((current) => !current);
  }, [setSidebarCollapsed]);

  const toggleTheme = useCallback(() => {
    setTheme((current) => (current === "dark" ? "light" : "dark"));
  }, [setTheme]);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const isTyping =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        target?.isContentEditable;

      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        openCommandPalette();
        return;
      }

      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "b") {
        event.preventDefault();
        toggleSidebar();
        return;
      }

      if (event.key === "/" && !isTyping) {
        event.preventDefault();
        focusGlobalSearch();
        return;
      }

      if (event.key === "Escape") {
        closeDrawer();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [closeDrawer, focusGlobalSearch, openCommandPalette, toggleSidebar]);

  useGSAP(
    () => {
      const shell = shellRef.current;

      if (!shell) {
        return;
      }

      const reduceMotion = window.matchMedia(
        "(prefers-reduced-motion: reduce)"
      ).matches;
      const collapse = sidebarCollapsed ? 1 : 0;
      const hasCollapsedChanged =
        previousSidebarCollapsedRef.current !== sidebarCollapsed;
      const shouldAnimate = animateSidebarRef.current && !reduceMotion;
      animateSidebarRef.current = false;
      previousSidebarCollapsedRef.current = sidebarCollapsed;
      gsap.killTweensOf(shell);

      if (!hasCollapsedChanged) {
        return;
      }

      if (!shouldAnimate) {
        gsap.set(shell, {
          "--sidebar-collapse": collapse,
        });
        return;
      }

      gsap.to(shell, {
        "--sidebar-collapse": collapse,
        duration: 0.28,
        ease: "power3.out",
      });
    },
    { dependencies: [sidebarCollapsed], scope: shellRef }
  );

  return (
    <div
      ref={shellRef}
      className="runtime-shell min-h-screen bg-(--background) text-(--foreground) lg:grid"
      style={
        {
          "--sidebar-collapse": initialCollapseRef.current,
          gridTemplateColumns: "var(--sidebar-width) minmax(0,1fr)",
        } as CSSProperties
      }
    >
      <aside
        aria-label="Runtime Console navigation"
        className="relative overflow-hidden border-(--border) bg-[color-mix(in_srgb,var(--sidebar)_92%,transparent)] lg:sticky lg:top-0 lg:h-screen lg:border-r max-lg:border-b"
      >
        <div className="h-11 border-b border-(--border) bg-(--chrome) max-lg:hidden">
          <div className="sidebar-header flex h-full items-center">
            <div
              aria-hidden={sidebarCollapsed}
              className="sidebar-copy flex min-w-0 items-center gap-2 overflow-hidden whitespace-nowrap"
            >
              <div className="grid h-5 min-w-11 place-items-center border border-[color-mix(in_srgb,var(--accent)_25%,transparent)] bg-(--accent-soft) px-1.5 text-(--accent) shadow-[0_0_18px_color-mix(in_srgb,var(--accent)_14%,transparent)]">
                <span className="font-mono text-[11px] font-semibold uppercase leading-none">
                  lenso
                </span>
              </div>
              <div
                aria-hidden={sidebarCollapsed}
                className="min-w-0 overflow-hidden whitespace-nowrap leading-tight"
              >
                <div className="font-mono text-[10px] uppercase tracking-[0.06em] text-(--secondary)">
                  Runtime
                </div>
                <div className="font-mono text-[10px] uppercase tracking-[0.06em] text-(--muted)">
                  Console
                </div>
              </div>
            </div>
            <button
              aria-label={
                sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"
              }
              className="grid size-6 shrink-0 place-items-center border border-(--border-subtle) bg-(--elevated) text-(--muted) transition hover:border-(--border) hover:text-(--foreground)"
              onClick={toggleSidebar}
              title={
                sidebarCollapsed
                  ? "Expand sidebar (Cmd/Ctrl+B)"
                  : "Collapse sidebar (Cmd/Ctrl+B)"
              }
              type="button"
            >
              {sidebarCollapsed ? (
                <PanelLeftOpen size={13} />
              ) : (
                <PanelLeftClose size={13} />
              )}
            </button>
          </div>
        </div>

        <nav className="p-2 max-lg:overflow-x-auto">
          <div className="grid gap-px max-lg:flex max-lg:min-w-max">
            {primaryNavItems.map((item) => (
              <NavLink key={item.to} {...item} />
            ))}
          </div>
          <div className="my-2 h-px bg-(--border-subtle) max-lg:hidden" />
          <div className="grid gap-px max-lg:hidden">
            <NavLink {...configNavItem} />
          </div>
        </nav>

        <div className="absolute right-0 bottom-0 left-0 border-t border-(--border-subtle) bg-[color-mix(in_srgb,var(--sidebar)_92%,transparent)] p-2 max-lg:hidden">
          <div className="sidebar-status-item flex w-full items-center gap-2 border border-(--border-subtle) bg-[color-mix(in_srgb,var(--surface)_55%,transparent)] px-2">
            <div className="size-1.5 shrink-0 rounded-full bg-(--success) shadow-[0_0_7px_var(--success)]" />
            <span
              aria-hidden={sidebarCollapsed}
              className="sidebar-copy overflow-hidden whitespace-nowrap font-mono text-[11px] uppercase tracking-[0.04em] text-(--foreground)"
            >
              Online
            </span>
            <span
              aria-hidden={sidebarCollapsed}
              className="sidebar-copy ml-auto overflow-hidden whitespace-nowrap font-mono text-[11px] text-(--muted)"
            >
              {runtimeConsoleDataSource()}
            </span>
          </div>
        </div>
      </aside>

      <main className="min-w-0">
        <header className="sticky top-0 z-20 grid min-h-11 grid-cols-[minmax(220px,520px)_1fr_auto_auto_auto_auto] items-center gap-2 border-b border-(--border) bg-(--chrome) px-3 shadow-[0_10px_32px_var(--shadow-soft)] backdrop-blur max-lg:grid-cols-[1fr_auto] max-lg:px-2 max-sm:block max-sm:space-y-2 max-sm:py-2">
          <RuntimeSearch />
          <div />
          <Button
            aria-label={
              theme === "dark" ? "Switch to light mode" : "Switch to dark mode"
            }
            className="theme-toggle-button border-(--border-subtle) bg-(--elevated) text-(--secondary) hover:border-(--border)"
            onClick={toggleTheme}
            title={
              theme === "dark" ? "Switch to light mode" : "Switch to dark mode"
            }
            variant="ghost"
          >
            {theme === "dark" ? (
              <Sun strokeWidth={1.9} />
            ) : (
              <Moon strokeWidth={1.9} />
            )}
          </Button>
          <Button
            className="max-sm:hidden"
            onClick={openCommandPalette}
            variant="ghost"
          >
            <Command size={13} />
            Command
            <span className="border border-(--border-subtle) px-1.5 py-0.5 font-mono text-[11px] text-(--muted)">
              ⌘K
            </span>
          </Button>
          <Badge className="h-7 rounded-none border-(--border) bg-(--elevated) font-mono text-[11px] text-(--secondary) max-lg:hidden">
            <Activity size={13} />
            local
          </Badge>
          <Badge className="h-7 rounded-none border-(--border) bg-(--elevated) font-mono text-[11px] text-(--secondary) max-lg:hidden">
            <Command size={13} />
            service:admin
          </Badge>
        </header>
        <div className="h-[calc(100vh-44px)] overflow-hidden">{children}</div>
      </main>
      <DetailDrawer onClose={closeDrawer} target={drawerTarget} />
      <RetryDialog />
      <CommandPalette onToggleTheme={toggleTheme} theme={theme} />
    </div>
  );
}

function NavLink({
  to,
  label,
  icon: Icon,
}: {
  to: string;
  label: string;
  icon: ComponentType<{ size?: number; strokeWidth?: number }>;
}) {
  return (
    <Link
      activeProps={{
        className:
          "bg-(--accent-soft) text-(--foreground) shadow-[inset_16px_0_24px_color-mix(in_srgb,var(--accent)_6%,transparent)]",
      }}
      aria-label={label}
      className="sidebar-nav-item flex h-7 w-full items-center gap-2 px-2 font-mono text-xs text-(--secondary) transition-colors hover:bg-(--hover) hover:text-(--foreground) max-lg:min-w-8 max-lg:justify-center max-lg:px-2"
      title={label}
      to={to}
    >
      <Icon size={13} strokeWidth={1.5} />
      <span className="sidebar-copy min-w-0 overflow-hidden whitespace-nowrap max-lg:hidden">
        {label}
      </span>
    </Link>
  );
}
