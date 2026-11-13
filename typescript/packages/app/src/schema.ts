/** Structural parser interface implemented by Zod and ordinary application validators. */
export interface Parser<T> {
  parse(value: unknown): T;
}

/** Input for operations without an argument, including JSON null at an external boundary. */
export const emptyInput: Parser<void> = {
  parse(value) {
    if (value !== null && value !== undefined) throw new TypeError("Expected no input");
  },
};
