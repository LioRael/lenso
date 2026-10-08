import { defineApp, definePlugin } from "lenso";
import { defineOperation, CliError } from "lenso-cli";
import { z } from "zod";

const input = z.strictObject({ value: z.string(), delay: z.number().optional() });
const fixture = definePlugin({
  id: "fixture",
  setup(context) {
    console.log("fixture setup");
    context.onCleanup(async () => {
      await Bun.sleep(20);
      console.log("fixture cleanup");
    });
    return {
      async echo(value: z.infer<typeof input>) {
        console.log({ token: "log-secret" });
        return { value: value.value, token: "result-secret" };
      },
      async slow(value: z.infer<typeof input>) {
        await Bun.sleep(value.delay ?? 100);
        console.log("fixture completed");
        return { value: value.value };
      },
      async denied(value: z.infer<typeof input>) {
        throw new CliError({
          code: "authorization-denied",
          phase: "invoke",
          message: "Operation is not authorized.",
          ...(value.value === "large-diagnostic" ? { details: "x".repeat(2000) } : {}),
        });
      },
      async failed(_value: z.infer<typeof input>) {
        throw new Error("private-secret stack");
      },
      async unsupported(_value: z.infer<typeof input>) {
        return undefined;
      },
      async big(_value: z.infer<typeof input>) {
        return { value: "x".repeat(2000) };
      },
      async hidden(_value: z.infer<typeof input>) {
        return { value: "never exposed" };
      },
    };
  },
});

const runtimeInput = {
  "~standard": {
    version: 1 as const,
    vendor: "fixture",
    validate: (value: unknown) => ({ value: value as z.infer<typeof input> }),
  },
};

const declaredOperations = (
  ["echo", "slow", "denied", "failed", "unsupported", "big", "hidden"] as const
).map((method) =>
  defineOperation({ plugin: fixture, method, input, description: `Fixture ${method}.` }),
);

// The runtime-only case is selected by a trusted test process, not tool input.
export const operations =
  process.env.MCP_TEST_RUNTIME_ONLY === "1"
    ? [{ ...declaredOperations[0]!, input: runtimeInput }]
    : declaredOperations;

console.log("fixture imported");
export default defineApp({ plugins: [fixture] });
