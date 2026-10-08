import { definePlugin } from "lenso/plugin";
import { greetingInput, GreetingInputError } from "./contracts";
export interface GreetingService {
  greet(input: { name: string }): Promise<{ message: string; count: number }>;
}
export const greeting = definePlugin<GreetingService>({
  id: "greeting",
  setup() {
    let count = 0;
    return {
      async greet({ name }) {
        const parsed = greetingInput.safeParse({ name });
        if (!parsed.success) throw new GreetingInputError();
        const trimmed = parsed.data.name;
        return { message: `Hello, ${trimmed}!`, count: ++count };
      },
    };
  },
});
