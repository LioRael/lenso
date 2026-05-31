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
import { EventsPage } from "../pages/events-page";
import { FunctionsPage } from "../pages/functions-page";
import { OverviewPage } from "../pages/overview-page";
import { TimelinePage } from "../pages/timeline-page";
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
  component: EventsPage,
});

const functionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/functions",
  component: FunctionsPage,
});

const timelineRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/timeline",
  component: TimelinePage,
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
      <section className="rounded-lg border border-white/10 bg-white/[0.035] p-8 shadow-2xl shadow-black/30">
        <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-slate-500">
          Coming later
        </p>
        <h1 className="text-2xl font-semibold text-slate-100">{title}</h1>
        <p className="mt-2 max-w-xl text-sm leading-6 text-slate-400">
          This prototype keeps the first slice focused on events, functions,
          timelines, and dead letters.
        </p>
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
  placeholderRoute("/queues", "Queues"),
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
