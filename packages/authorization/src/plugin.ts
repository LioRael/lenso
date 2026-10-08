import { definePlugin, type Plugin, type PluginContext } from "@lenso/core/plugin";
import type { Authorization, Resource } from "./types";

export interface AuthorizationPluginOptions<A extends string, R extends Resource, C> {
  readonly id: string;
  /** Preserve the exact dependency instances supplied by the application. */
  readonly requires?: readonly Plugin<unknown>[];
  /** Build the service during setup; any resources remain application-owned. */
  readonly setup: (
    context: PluginContext,
  ) => Authorization<A, R, C> | Promise<Authorization<A, R, C>>;
}

/** A thin lifecycle adapter; it acquires and closes no resources itself. */
export function createAuthorizationPlugin<
  A extends string,
  R extends Resource = Resource,
  C = unknown,
>(options: AuthorizationPluginOptions<A, R, C>): Plugin<Authorization<A, R, C>> {
  return definePlugin({
    id: options.id,
    requires: options.requires,
    setup: options.setup,
  });
}
