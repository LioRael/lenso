import { defineTask } from "@lenso/tasks";
import { z } from "zod";

export const fixtureTask = defineTask({
  name: "schedulerFixture",
  input: z.object({ value: z.number(), failUntil: z.number().default(0) }).strict(),
  maxAttempts: 2,
  retry: { delaySeconds: 1, backoff: false },
  async handler(input, context) {
    if (context.attempt <= input.failUntil) throw new Error("fixture failure not stored");
    return { value: input.value, attempt: context.attempt };
  },
  result: (value) => value,
});
