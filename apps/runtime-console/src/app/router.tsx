import {
  Outlet,
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
} from "@tanstack/react-router";

import { RuntimeConsoleProvider } from "../components/runtime/runtime-console-context";
import { RuntimeConsoleShell } from "../components/runtime/runtime-console-shell";
import { ConfigPage } from "../pages/config-page";
import { DataPage } from "../pages/data-page";
import { DeadLettersPage } from "../pages/dead-letters-page";
import { OverviewPage } from "../pages/overview-page";
import { QueuesPage } from "../pages/queues-page";
import { RemoteProxyCallsPage } from "../pages/remote-proxy-calls-page";
import { RuntimeStoriesPage } from "../pages/runtime-stories-page";

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
    throw redirect({ to: "/runtime/stories" });
  },
});

const storiesWorkbenchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/runtime/stories",
  component: RuntimeStoriesPage,
});

const legacyStoriesAliasRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/runtime/traces",
  beforeLoad: () => {
    throw redirect({ to: "/runtime/stories" });
  },
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
    throw redirect({ to: "/runtime/stories" });
  },
});

const functionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/functions",
  beforeLoad: () => {
    throw redirect({ to: "/runtime/stories" });
  },
});

const timelineRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/timeline",
  beforeLoad: () => {
    throw redirect({ to: "/runtime/stories" });
  },
});

const deadLettersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dead-letters",
  component: DeadLettersPage,
});

const remoteProxyCallsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/remote-proxy-calls",
  component: RemoteProxyCallsPage,
});

const configRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/config",
  component: ConfigPage,
});

const dataRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/data",
  component: DataPage,
});

const placeholderRoute = (path: string, title: string) =>
  createRoute({
    getParentRoute: () => rootRoute,
    path,
    component: () => (
      <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
        <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
          <div className="flex items-center gap-2">
            <h1 className="font-mono text-[13px] font-semibold">{title}</h1>
            <span className="ml-auto font-mono text-[10px] text-(--muted)">
              deferred
            </span>
          </div>
        </header>
        <div className="p-3 font-mono">
          <div className="border-y border-(--border-subtle)">
            <div className="grid grid-cols-[96px_minmax(0,1fr)] border-b border-(--border-subtle) text-[11px]">
              <div className="bg-(--sidebar) px-3 py-1.5 text-(--muted)">
                status
              </div>
              <div className="px-3 py-1.5 text-(--secondary)">
                future runtime
              </div>
            </div>
            <div className="grid grid-cols-[96px_minmax(0,1fr)] text-[11px]">
              <div className="bg-(--sidebar) px-3 py-1.5 text-(--muted)">
                reason
              </div>
              <div className="px-3 py-1.5 text-(--secondary)">
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
  storiesWorkbenchRoute,
  legacyStoriesAliasRoute,
  overviewRoute,
  eventsRoute,
  functionsRoute,
  timelineRoute,
  deadLettersRoute,
  remoteProxyCallsRoute,
  createRoute({
    getParentRoute: () => rootRoute,
    path: "/queues",
    component: QueuesPage,
  }),
  configRoute,
  dataRoute,
  placeholderRoute("/flows", "Flows"),
  placeholderRoute("/agents", "Agents"),
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
