import type { StandardJSONSchemaV1, StandardSchemaV1 } from "@standard-schema/spec";
import { validateValue } from "@lenso/runtime-sdk/schema";
import type { AggregateInput } from "../.lenso/runtime/logs";
import { v1Contract as logsContract } from "./runtime-contract";

// CLI discovery and validation share the generated wire contract, not a second schema.
export const logsInput: StandardSchemaV1<AggregateInput, AggregateInput> &
  StandardJSONSchemaV1<AggregateInput, AggregateInput> = {
  "~standard": {
    version: 1,
    vendor: "@lenso/runtime-sdk",
    validate(value) {
      try {
        validateValue(logsContract.methods.aggregate.input, value);
        return { value: value as AggregateInput };
      } catch {
        return { issues: [{ message: "Input does not match the log aggregation contract." }] };
      }
    },
    jsonSchema: {
      input: () => ({ ...logsContract.methods.aggregate.input }),
      output: () => ({ ...logsContract.methods.aggregate.input }),
    },
  },
};
