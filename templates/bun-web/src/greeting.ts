import { bindConfig, definePluginConfig, type ConfigSource } from "@lenso/core/config";
import { z } from "zod";

export const greetingInput = z.object({ name: z.string().trim().min(2) });

export function createGreetingService(prefix = "Hello") {
  let count = 0;
  return {
    async greet(input: z.output<typeof greetingInput>) {
      const { name } = greetingInput.parse(input);
      return { message: `${prefix}, ${name}!`, count: ++count };
    },
  };
}

export const greetingConfig = definePluginConfig({
  schema: z.strictObject({ prefix: z.string().min(1).default("Hello") }),
});

export function createGreetingPlugin(sources: readonly ConfigSource[] = []) {
  return bindConfig(greetingConfig, sources, {
    id: "greeting",
    setup: (_context, config) => createGreetingService(config.prefix),
  });
}
