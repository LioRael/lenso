import { defineApp } from "@lenso/core";
import { defineOperation } from "@lenso/cli";
import { greetingInput } from "@app/greeting";
import { greeting } from "./src/application";

export default defineApp({ plugins: [greeting] });
export const operations = [
  defineOperation({
    plugin: greeting,
    method: "greet",
    input: greetingInput,
    description: "Greet a person using the shared service",
    effect: "write",
  }),
];
