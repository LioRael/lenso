import { Link } from "@tanstack/react-router";
import {
  Activity,
  Boxes,
  Braces,
  CircleDot,
  Command,
  Cpu,
  GitBranch,
  Inbox,
  Settings,
  Sparkles,
  TriangleAlert,
  Workflow,
} from "lucide-react";
import { useEffect } from "react";
import type { ComponentType, PropsWithChildren } from "react";

import { Badge } from "../ui/badge";
import { Button } from "../ui/button";
import { CommandPalette } from "./command-palette";
import { DetailDrawer } from "./detail-drawer";
import { RetryDialog } from "./retry-dialog";
import { useRuntimeConsole } from "./runtime-console-context";
import { RuntimeSearch } from "./runtime-search";

const primaryNavItems = [
  { to: "/runtime/traces", label: "Traces", icon: Workflow },
  { to: "/events", label: "Events", icon: CircleDot },
  { to: "/functions", label: "Functions", icon: Cpu },
  { to: "/timeline", label: "Timeline", icon: GitBranch },
  { to: "/queues", label: "Queues", icon: Inbox },
  { to: "/flows", label: "Flows", icon: Boxes },
  { to: "/agents", label: "Agents", icon: Sparkles },
  { to: "/dead-letters", label: "Dead Letters", icon: TriangleAlert },
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
  }, [closeDrawer, focusGlobalSearch, openCommandPalette]);

  return (
    <div className="min-h-screen bg-[#050609] text-slate-100 lg:grid lg:grid-cols-[184px_minmax(0,1fr)]">
      <aside
        aria-label="Runtime Console navigation"
        className="border-white/10 bg-[#07080a] lg:sticky lg:top-0 lg:h-screen lg:border-r lg:p-2 max-lg:border-b max-lg:px-2 max-lg:py-1.5"
      >
        <div className="flex items-center gap-2 px-1.5 py-1 pb-3 max-lg:hidden">
          <div className="grid size-6 place-items-center border border-cyan-300/25 bg-cyan-300/10 text-cyan-200">
            <Braces size={14} />
          </div>
          <div>
            <div className="font-mono text-[11px] font-semibold uppercase tracking-[0.05em] text-slate-100">
              Lenso Runtime
            </div>
            <div className="font-mono text-[10px] text-slate-600">
              trace workbench
            </div>
          </div>
        </div>

        <nav className="max-lg:overflow-x-auto">
          <div className="grid gap-0.5 py-1 max-lg:flex max-lg:min-w-max">
            {primaryNavItems.map((item) => (
              <NavLink key={item.to} {...item} />
            ))}
          </div>
          <div className="mx-1 my-2 h-px bg-white/10 max-lg:hidden" />
          <div className="grid gap-0.5 py-1 max-lg:hidden">
            <NavLink {...settingsNavItem} />
          </div>
        </nav>
      </aside>

      <main className="min-w-0">
        <header className="sticky top-0 z-20 grid min-h-12 grid-cols-[minmax(220px,520px)_1fr_auto_auto_auto] items-center gap-2 border-b border-white/10 bg-[#06070a]/90 px-3 max-lg:grid-cols-[1fr_auto] max-lg:px-2 max-sm:block max-sm:space-y-2 max-sm:py-2">
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
          <Badge className="h-7 rounded-none border-white/10 bg-white/[0.025] font-mono text-[10px] max-lg:hidden">
            <Activity size={13} />
            local
          </Badge>
          <Badge className="h-7 rounded-none border-white/10 bg-white/[0.025] font-mono text-[10px] max-lg:hidden">
            <Command size={13} />
            service:admin
          </Badge>
        </header>
        <div className="mx-auto max-w-[1720px] px-2 py-2 pb-4 max-sm:px-2">
          {children}
        </div>
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
}: {
  to: string;
  label: string;
  icon: ComponentType<{ size?: number }>;
}) {
  return (
    <Link
      activeProps={{
        className: "bg-cyan-300/[0.07] text-slate-100 border-cyan-300/20",
      }}
      className="flex min-h-7 items-center gap-2 border border-transparent px-2 font-mono text-[11px] text-slate-500 transition hover:bg-white/[0.04] hover:text-slate-200 max-lg:min-w-8 max-lg:justify-center max-lg:px-2"
      to={to}
    >
      <Icon size={13} />
      <span className="max-lg:hidden">{label}</span>
    </Link>
  );
}
