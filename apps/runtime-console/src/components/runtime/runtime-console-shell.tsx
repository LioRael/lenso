import { Link } from "@tanstack/react-router";
import {
  Activity,
  Boxes,
  Command,
  Inbox,
  PanelLeftClose,
  PanelLeftOpen,
  Settings,
  Sparkles,
  TriangleAlert,
  Workflow,
} from "lucide-react";
import { useEffect } from "react";
import type { ComponentType, PropsWithChildren } from "react";

import { usePersistedLayout } from "../../hooks/use-persisted-layout";
import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { CommandPalette } from "./command-palette";
import { DetailDrawer } from "./detail-drawer";
import { RetryDialog } from "./retry-dialog";
import { useRuntimeConsole } from "./runtime-console-context";
import { RuntimeSearch } from "./runtime-search";

const primaryNavItems = [
  { to: "/runtime/traces", label: "Traces", icon: Workflow },
  { to: "/dead-letters", label: "Dead Letters", icon: TriangleAlert },
  { to: "/queues", label: "Queues", icon: Inbox },
  { to: "/overview", label: "Overview", icon: Activity },
] as const;

const settingsNavItem = {
  to: "/settings",
  label: "Settings",
  icon: Settings,
} as const;

export function RuntimeConsoleShell({ children }: PropsWithChildren) {
  const { closeDrawer, drawerTarget, focusGlobalSearch, openCommandPalette } =
    useRuntimeConsole();
  const [sidebarCollapsed, setSidebarCollapsed] = usePersistedLayout(
    "runtime-console:sidebar-collapsed",
    false
  );

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
        setSidebarCollapsed((current) => !current);
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
  }, [closeDrawer, focusGlobalSearch, openCommandPalette, setSidebarCollapsed]);

  return (
    <div
      className="min-h-screen bg-black text-[#f4f4f4] lg:grid"
      style={{
        gridTemplateColumns: `${sidebarCollapsed ? 52 : 228}px minmax(0,1fr)`,
      }}
    >
      <aside
        aria-label="Runtime Console navigation"
        className="relative overflow-hidden border-[#2d2d2d] bg-[#0a0a0a] lg:sticky lg:top-0 lg:h-screen lg:border-r max-lg:border-b"
      >
        <div className="flex items-center justify-between border-b border-[#1d1d1d] px-3 py-2.5 max-lg:hidden">
          <div className="flex min-w-0 items-center gap-2">
            <div className="grid size-5 place-items-center text-[#f3f724]">
              <span className="font-mono text-[13px] leading-none">iii</span>
            </div>
            <div
              className={`min-w-0 leading-tight ${sidebarCollapsed ? "hidden" : ""}`}
            >
              <div className="font-mono text-[9px] uppercase tracking-[0.08em] text-[#9ca3af]">
                Runtime
              </div>
              <div className="font-mono text-[9px] uppercase tracking-[0.08em] text-[#5b5b5b]">
                Console
              </div>
            </div>
          </div>
          <button
            aria-label={
              sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"
            }
            className="grid size-6 flex-shrink-0 place-items-center border border-[#1d1d1d] bg-[#111111] text-[#5b5b5b] transition hover:text-[#f4f4f4]"
            onClick={() => setSidebarCollapsed((current) => !current)}
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

        <nav className="p-2 max-lg:overflow-x-auto">
          <div className="grid gap-px max-lg:flex max-lg:min-w-max">
            {primaryNavItems.map((item) => (
              <NavLink collapsed={sidebarCollapsed} key={item.to} {...item} />
            ))}
          </div>
          <div className="mt-3 border-t border-[#1d1d1d] pt-2 max-lg:hidden">
            <div
              className={`mb-1 px-2 font-mono text-[9px] uppercase tracking-[0.08em] text-[#3d3d3d] ${
                sidebarCollapsed ? "sr-only" : ""
              }`}
            >
              Future
            </div>
            <div className="grid gap-px">
              <DisabledNav
                collapsed={sidebarCollapsed}
                label="Flows"
                icon={Boxes}
              />
              <DisabledNav
                collapsed={sidebarCollapsed}
                label="Agents"
                icon={Sparkles}
              />
            </div>
          </div>
          <div className="my-2 h-px bg-[#1d1d1d] max-lg:hidden" />
          <div className="grid gap-px max-lg:hidden">
            <NavLink collapsed={sidebarCollapsed} {...settingsNavItem} />
          </div>
        </nav>

        <div className="absolute right-0 bottom-0 left-0 border-t border-[#1d1d1d] p-2 max-lg:hidden">
          <div
            className={`flex items-center gap-2 border border-[#1d1d1d] px-2 py-1.5 ${
              sidebarCollapsed ? "justify-center" : ""
            }`}
          >
            <div className="size-1.5 rounded-full bg-[#22c55e] shadow-[0_0_6px_#22c55e]" />
            <span
              className={`font-mono text-[10px] uppercase tracking-[0.04em] text-[#f4f4f4] ${
                sidebarCollapsed ? "hidden" : ""
              }`}
            >
              Online
            </span>
            <span
              className={`ml-auto font-mono text-[10px] text-[#5b5b5b] ${
                sidebarCollapsed ? "hidden" : ""
              }`}
            >
              mock
            </span>
          </div>
        </div>
      </aside>

      <main className="min-w-0">
        <header className="sticky top-0 z-20 grid min-h-11 grid-cols-[minmax(220px,520px)_1fr_auto_auto_auto] items-center gap-2 border-b border-[#2d2d2d] bg-black px-3 max-lg:grid-cols-[1fr_auto] max-lg:px-2 max-sm:block max-sm:space-y-2 max-sm:py-2">
          <RuntimeSearch />
          <div />
          <Button
            className="max-sm:hidden"
            onClick={openCommandPalette}
            variant="ghost"
          >
            <Command size={13} />
            Command
            <span className="rounded-md border border-white/10 px-1.5 py-0.5 font-mono text-[11px] text-slate-500">
              ⌘K
            </span>
          </Button>
          <Badge className="h-7 rounded-full border-[#2d2d2d] bg-[#111111] font-mono text-[10px] text-[#9ca3af] max-lg:hidden">
            <Activity size={13} />
            local
          </Badge>
          <Badge className="h-7 rounded-full border-[#2d2d2d] bg-[#111111] font-mono text-[10px] text-[#9ca3af] max-lg:hidden">
            <Command size={13} />
            service:admin
          </Badge>
        </header>
        <div className="h-[calc(100vh-44px)] overflow-hidden">{children}</div>
      </main>
      <DetailDrawer onClose={closeDrawer} target={drawerTarget} />
      <RetryDialog />
      <CommandPalette />
    </div>
  );
}

function NavLink({
  to,
  label,
  icon: Icon,
  collapsed,
}: {
  to: string;
  label: string;
  icon: ComponentType<{ size?: number; strokeWidth?: number }>;
  collapsed: boolean;
}) {
  return (
    <Link
      activeProps={{
        className: "border-l-[#f3f724] bg-[#f3f724]/[0.055] text-[#f4f4f4]",
      }}
      className={`flex h-7 items-center gap-2 border-l-2 border-l-transparent px-2 font-mono text-[11px] text-[#9ca3af] transition hover:bg-[#111111] hover:text-[#f4f4f4] max-lg:min-w-8 max-lg:justify-center max-lg:px-2 ${
        collapsed ? "justify-center" : ""
      }`}
      title={collapsed ? label : undefined}
      to={to}
    >
      <Icon size={13} strokeWidth={1.5} />
      <span className={collapsed ? "sr-only" : "max-lg:hidden"}>{label}</span>
    </Link>
  );
}

function DisabledNav({
  label,
  icon: Icon,
  collapsed,
}: {
  label: string;
  icon: ComponentType<{ size?: number; strokeWidth?: number }>;
  collapsed: boolean;
}) {
  return (
    <div
      className={`flex h-7 items-center gap-2 border-l-2 border-l-transparent px-2 font-mono text-[11px] text-[#3d3d3d] ${
        collapsed ? "justify-center" : ""
      }`}
      title={collapsed ? `${label} later` : undefined}
    >
      <Icon size={13} strokeWidth={1.5} />
      <span className={collapsed ? "sr-only" : ""}>{label}</span>
      <span className={`ml-auto text-[9px] ${collapsed ? "hidden" : ""}`}>
        later
      </span>
    </div>
  );
}
