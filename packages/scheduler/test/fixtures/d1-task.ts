import type { D1Database } from "@cloudflare/workers-types";
import { defineTask } from "@lenso/tasks";
import { z } from "zod";

export function createD1FixtureTask(database: D1Database) {
  return defineTask({
    name: "d1Fixture",
    input: z.object({ value: z.number().finite(), failUntil: z.number().default(0) }).strict(),
    maxAttempts: 2,
    retry: { delaySeconds: 1, backoff: false },
    async handler(input, context) {
      if (context.attempt <= input.failUntil) throw new Error("fixture failure");
      const permission = await database
        .prepare("SELECT allowed FROM fixture_permission WHERE subject_id = ?")
        .bind("alice")
        .first<{ allowed: number }>();
      if (!permission?.allowed) throw new Error("fixture execution denied");
      context.signal.throwIfAborted();
      await database
        .prepare(
          "INSERT INTO fixture_effect(job_id, value) VALUES (?, ?) ON CONFLICT(job_id) DO NOTHING",
        )
        .bind(context.jobId, input.value)
        .run();
      return { value: input.value, attempt: context.attempt };
    },
    result: (value) => value,
  });
}
