import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { createPayments, type PaymentsRuntime } from "./index";
import type { PaymentsAuthorization, PaymentsProvider, PaymentsStore } from "./contracts";
import type { PaymentsConfigBinding } from "./config";

export function createPaymentsPlugin<A>(options: {
  id: string;
  store: Plugin<PaymentsStore>;
  provider: Plugin<PaymentsProvider>;
  authorization: Plugin<PaymentsAuthorization<A>>;
  config?: PaymentsConfigBinding;
}): Plugin<PaymentsRuntime<A>> {
  return definePlugin({
    id: options.id,
    requires: [options.store, options.provider, options.authorization],
    config: options.config,
    setup(context) {
      const config = options.config ? context.config!(options.config) : undefined;
      // These services and their clients are borrowed. Their exact installed owners close them.
      return createPayments({
        store: context.get(options.store),
        provider: context.get(options.provider),
        authorize: context.get(options.authorization),
        ...config,
      });
    },
  });
}
