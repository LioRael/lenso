import { bindConfig, defineApp, definePluginConfig } from "@lenso/core";
import { envSource } from "@lenso/core/config/env";
import { defineOperation } from "@lenso/cli";
import { z } from "zod";

const greetingInput = z.object({ name: z.string().trim().min(2) });

export async function greet(input: z.output<typeof greetingInput>, prefix = "Hello") {
  const { name } = greetingInput.parse(input);
  return { message: `${prefix}, ${name}!` };
}

const greeting = bindConfig(
  definePluginConfig({
    schema: z.strictObject({ prefix: z.string().min(1).default("Hello") }),
  }),
  [
    envSource({
      id: "deployment",
      read: (name) => process.env[name],
      bindings: { prefix: { name: "GREETING_PREFIX" } },
    }),
  ],
  {
    id: "greeting",
    setup(_context, config) {
      return { greet: (input: z.output<typeof greetingInput>) => greet(input, config.prefix) };
    },
  },
);

export default defineApp({ plugins: [greeting] });
export const operations = [
  defineOperation({
    plugin: greeting,
    method: "greet",
    input: greetingInput,
    description: "Greet a person",
    effect: "read",
  }),
];
