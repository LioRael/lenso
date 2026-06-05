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
import { FunctionsPage } from "../pages/functions-page";
import { ModulesPage } from "../pages/modules-page";
import { OperationsPage } from "../pages/operations-page";
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

const overviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/overview",
  component: OverviewPage,
});

const operationsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/operations",
  beforeLoad: () => {
    throw redirect({ to: "/operations/queues" });
  },
});

const operationsQueuesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/operations/queues",
  component: () => (
    <OperationsPage active="queues">
      <QueuesPage />
    </OperationsPage>
  ),
});

const operationsDeadLettersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/operations/dead-letters",
  component: () => (
    <OperationsPage active="dead-letters">
      <DeadLettersPage />
    </OperationsPage>
  ),
});

const operationsFunctionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/operations/functions",
  component: () => (
    <OperationsPage active="functions">
      <FunctionsPage />
    </OperationsPage>
  ),
});

const operationsRemoteCallsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/operations/remote-calls",
  component: () => (
    <OperationsPage active="remote-calls">
      <RemoteProxyCallsPage />
    </OperationsPage>
  ),
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

const modulesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/modules",
  component: ModulesPage,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  storiesWorkbenchRoute,
  overviewRoute,
  operationsRoute,
  operationsQueuesRoute,
  operationsDeadLettersRoute,
  operationsFunctionsRoute,
  operationsRemoteCallsRoute,
  modulesRoute,
  configRoute,
  dataRoute,
]);

export const router = createRouter({ routeTree });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
