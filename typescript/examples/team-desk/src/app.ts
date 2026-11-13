import { Catalog, ConsoleExtension, Executor, defineAction, emptyInput } from "@lenso/app";
import type { ConsolePageDefinition } from "@lenso/app";
import { Store } from "./store.js";
import { membersModule } from "./members.js";
import { projectsModule } from "./projects.js";
import { itemsModule } from "./items.js";
import { notificationsModule } from "./notifications.js";

export function createApplication(store: Store) {
  const members = membersModule(), projects = projectsModule(), items = itemsModule(), notifications = notificationsModule();
  items.changed.subscribe("inbox", notifications.deliver, (change) => change);
  const executor = new Executor((record) => {
    try { store.record(record); } catch (error) { console.error("Could not persist call record", error); }
  });
  const catalog = new Catalog();
  const modules = [members, projects, items, notifications];
  for (const contribution of modules) catalog.register(contribution.module);
  catalog.requireResources(["sqlite"]);
  const consoleExtension = new ConsoleExtension(executor, modules.flatMap<ConsolePageDefinition<Store>>((contribution) => contribution.pages), [members.actions.session, notifications.actions.drain]);
  const manifest = defineAction<Store>()({
    id: "console.manifest", input: emptyInput,
    authorize(context) { context.resources.access(context); },
    run() { return consoleExtension.manifest(catalog); },
  });
  return { executor, catalog, consoleExtension, manifest, modules: { members, projects, items, notifications } };
}
