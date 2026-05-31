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
  LayoutDashboard,
  Settings,
  Sparkles,
  TriangleAlert,
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
  { to: "/overview", label: "Overview", icon: LayoutDashboard },
  { to: "/events", label: "Events", icon: CircleDot },
  { to: "/functions", label: "Functions", icon: Cpu },
  { to: "/timeline", label: "Timeline", icon: GitBranch },
  { to: "/queues", label: "Queues", icon: Inbox },
  { to: "/flows", label: "Flows", icon: Boxes },
  { to: "/agents", label: "Agents", icon: Sparkles },
  { to: "/dead-letters", label: "Dead Letters", icon: TriangleAlert },
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
    <div className="min-h-screen bg-transparent text-slate-100 lg:grid lg:grid-cols-[236px_minmax(0,1fr)]">
      <aside
        aria-label="Runtime Console navigation"
        className="border-white/10 bg-black/35 backdrop-blur-xl lg:sticky lg:top-0 lg:h-screen lg:border-r lg:p-3.5 max-lg:border-b max-lg:px-3 max-lg:py-2"
      >
        <div className="flex items-center gap-2.5 px-2 py-1 pb-4 max-lg:hidden">
          <div className="grid size-7 place-items-center rounded-lg border border-blue-300/30 bg-blue-300/10 text-blue-200">
            <Braces size={16} />
          </div>
          <div>
            <div className="text-[13px] font-semibold text-slate-100">
              Lenso Runtime
            </div>
            <div className="text-[11px] text-slate-500">
              local command center
            </div>
          </div>
        </div>

        <nav className="max-lg:overflow-x-auto">
          <div className="grid gap-1 py-2 max-lg:flex max-lg:min-w-max">
            {primaryNavItems.map((item) => (
              <NavLink key={item.to} {...item} />
            ))}
          </div>
          <div className="mx-1 my-2 h-px bg-white/10 max-lg:hidden" />
          <div className="grid gap-1 py-2 max-lg:hidden">
            <NavLink {...settingsNavItem} />
          </div>
        </nav>
      </aside>

      <main className="min-w-0">
        <header className="sticky top-0 z-20 grid min-h-16 grid-cols-[minmax(220px,560px)_1fr_auto_auto_auto] items-center gap-3 border-b border-white/10 bg-[#08090c]/75 px-6 backdrop-blur-xl max-lg:grid-cols-[1fr_auto] max-lg:px-3 max-sm:block max-sm:space-y-2 max-sm:py-3">
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
          <Badge className="max-lg:hidden">
            <Activity size={13} />
            local
          </Badge>
          <Badge className="max-lg:hidden">
            <Command size={13} />
            service:admin
          </Badge>
        </header>
        <div className="mx-auto max-w-[1480px] px-7 py-6 pb-12 max-sm:px-3.5">
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
        className: "bg-white/[0.07] text-slate-100",
      }}
      className="flex min-h-8 items-center gap-2.5 rounded-lg px-2.5 text-[13px] text-slate-400 transition hover:bg-white/[0.055] hover:text-slate-100 max-lg:min-w-8 max-lg:justify-center max-lg:px-2"
      to={to}
    >
      <Icon size={15} />
      <span className="max-lg:hidden">{label}</span>
    </Link>
  );
}
