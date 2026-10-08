import { defineApp } from "lenso";
import { defineOperation } from "lenso-cli";
import { greetingInput } from "./src/contracts";
import { greeting } from "./src/greeting";
export default defineApp({ plugins: [greeting] });

export const operations = [
  defineOperation({
    plugin: greeting,
    method: "greet",
    input: greetingInput,
    description: "Greet a name and increment this app instance's in-memory count.",
    effect: "write",
    source: { file: "src/greeting.ts", export: "greeting" },
  }),
];
