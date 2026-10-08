import { defineApp } from "@lenso/core";
import { defineOperation } from "@lenso/cli";
import { greeting, greetingInput } from "./src/server";
export default defineApp({ plugins: [greeting] });
export const operations = [
  defineOperation({
    plugin: greeting,
    method: "greet",
    input: greetingInput,
    description: "Greet a person using the Web application's service",
    effect: "write",
  }),
];
