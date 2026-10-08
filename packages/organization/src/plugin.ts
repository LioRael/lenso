import { definePlugin, type Plugin } from "@lenso/core/plugin";
import type { OrganizationAccess, OrganizationConfig, OrganizationStore } from "./contracts";
import { createOrganizationService, type OrganizationService } from "./service";

/** Borrow exact database/access instances; their owners retain cleanup. */
export function createOrganizationPlugin<Database, Actor>(options: {
  id: string;
  database: Plugin<Database>;
  access: Plugin<OrganizationAccess<Actor>>;
  store(database: Database): OrganizationStore;
  config?: Partial<OrganizationConfig>;
}): Plugin<OrganizationService<Actor>> {
  return definePlugin({
    id: options.id,
    requires: [options.database, options.access],
    setup(context) {
      return createOrganizationService({
        store: options.store(context.get(options.database)),
        access: context.get(options.access),
        config: options.config,
      });
    },
  });
}
