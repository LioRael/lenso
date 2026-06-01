import {
  Outlet,
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
} from "@tanstack/react-router";

import { RuntimeConsoleProvider } from "../components/runtime/runtime-console-context";
import { RuntimeConsoleShell } from "../components/runtime/runtime-console-shell";
import { DeadLettersPage } from "../pages/dead-letters-page";
import { OverviewPage } from "../pages/overview-page";
import { QueuesPage } from "../pages/queues-page";
import { TraceWorkbenchPage } from "../pages/trace-workbench-page";

const rootRoute = createRootRoute({
  component: () => (
    <RuntimeConsoleProvider>
      <RuntimeConsoleShell>
        <Outlet />
      </RuntimeConsoleShell>
    </RuntimeConsoleProvider>
  ),
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/runtime/traces" });
  },
});

const traceWorkbenchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/runtime/traces",
  component: TraceWorkbenchPage,
});

const overviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/overview",
  component: OverviewPage,
});

const eventsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/events",
  beforeLoad: () => {
    throw redirect({ to: "/runtime/traces" });
  },
});

const functionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/functions",
  beforeLoad: () => {
    throw redirect({ to: "/runtime/traces" });
  },
});

const timelineRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/timeline",
  beforeLoad: () => {
    throw redirect({ to: "/runtime/traces" });
  },
});

const deadLettersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dead-letters",
  component: DeadLettersPage,
});

const placeholderRoute = (path: string, title: string) =>
  createRoute({
    getParentRoute: () => rootRoute,
    path,
    component: () => (
      <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-black text-[#f4f4f4]">
        <header className="border-b border-[#1d1d1d] bg-[#0a0a0a] px-3 py-2">
          <div className="flex items-center gap-2">
            <h1 className="font-mono text-[13px] font-semibold">{title}</h1>
            <span className="ml-auto font-mono text-[10px] text-[#5b5b5b]">
              deferred
            </span>
          </div>
        </header>
        <div className="p-3 font-mono">
          <div className="border-y border-[#1d1d1d]">
            <div className="grid grid-cols-[96px_minmax(0,1fr)] border-b border-[#1d1d1d] text-[11px]">
              <div className="bg-[#080808] px-3 py-1.5 text-[#5b5b5b]">
                status
              </div>
              <div className="px-3 py-1.5 text-[#9ca3af]">future runtime</div>
            </div>
            <div className="grid grid-cols-[96px_minmax(0,1fr)] text-[11px]">
              <div className="bg-[#080808] px-3 py-1.5 text-[#5b5b5b]">
                reason
              </div>
              <div className="px-3 py-1.5 text-[#9ca3af]">
                no backend support in mock mode
              </div>
            </div>
          </div>
        </div>
      </section>
    ),
  });

const routeTree = rootRoute.addChildren([
  indexRoute,
  traceWorkbenchRoute,
  overviewRoute,
  eventsRoute,
  functionsRoute,
  timelineRoute,
  deadLettersRoute,
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/queues",
    component: QueuesPage,
  }),
  placeholderRoute("/flows", "Flows"),
  placeholderRoute("/agents", "Agents"),
  placeholderRoute("/settings", "Settings"),
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
