import { expect, test } from "bun:test";
import { ProcessorError } from "@lenso/media";
import { createBunImageProcessor } from "@lenso/media/bun";

test("compiled root and Bun subpath share the domain error constructor", async () => {
  try {
    await createBunImageProcessor().inspect(new Uint8Array([0]), new AbortController().signal);
    throw new Error("Expected unsupported input");
  } catch (error) {
    expect(error).toBeInstanceOf(ProcessorError);
    expect((error as ProcessorError).code).toBe("unsupported-image");
  }
});
