import { defineApp, definePlugin } from "@lenso/core";
import { defineOperation } from "@lenso/cli";
import { z } from "zod";

const greetingInput = z.object({ name: z.string().trim().min(2) });

const greeting = definePlugin({
  id: "greeting",
  setup: () => ({
    async greet(input: z.output<typeof greetingInput>) {
      const { name } = greetingInput.parse(input);
      return { message: `${process.env.GREETING_PREFIX ?? "Hello"}, ${name}!` };
    },
  }),
});

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
