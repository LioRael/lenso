import { z } from "zod";

export const greetingInput = z.object({
  name: z.string().trim().min(2, "Name must contain at least 2 characters"),
});
export class GreetingInputError extends Error {
  readonly code = "invalid-name";
  constructor() {
    super("Name must contain at least 2 characters");
  }
}
