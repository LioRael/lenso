# Optional Action and Module metadata

This feature adds public descriptions and field-level input failures to the native SDK contract. It requires App 0.1.2 and Console client 3.1.0 or newer. App 0.1.1 and Console 3.0.0 do not include these APIs. SQLite packages retain version 0.1.1 and remain compatible.

## One description, native execution

An Action can declare `description`, `inputSchema`, `outputSchema`, `capabilities` and `effects`. A Module can declare `description`, `version`, `capabilities` and `effects`. Fields are optional. Schemas can carry field documentation through JSON Schema's `description` keyword.

Descriptions are advisory public data. Native parsers, invariant hooks, Access policies, resources and Executor remain authoritative. A schema does not validate a call, a capability does not grant permission, and an effect declaration does not prove an effect occurred. Undeclared metadata is `null` in an Action description; absent capability/effect lists mean unknown, while `[]` explicitly declares none.

In TypeScript, reuse a parser's own schema exporter when available:

```ts
const input = z.object({ name: z.string().min(1).describe("Display name") });
const action = defineAction<Resources>();
const create = action({
  id: "projects.create",
  input,
  metadata: {
    description: "Create a project",
    inputSchema: z.toJSONSchema(input, { io: "input" }),
    capabilities: ["team.write"],
    effects: ["write:projects"],
  },
  authorize(context) { context.resources.requireWrite(context); },
  run(context, value) { return context.resources.create(value); },
});
const projects = new Module("projects", { actions: { create } }, { version: "1" });
```

Zod remains an application choice, not a core SDK dependency. `JsonSchema` accepts a boolean or an open object so generated schemas can be used directly. Authors own JSON serializability and the accuracy of annotations, including alignment with any external codec representation. Native values need no schemas or JSON conversion.

In Rust, `Action::metadata()` defaults to `None`. Existing implementations need no new method or serde bound. Opt in with a returned owned `ActionMetadata`, and annotate a Module with `with_metadata(ModuleMetadata { .. })`. `input_schema` and `output_schema` are available under the optional `json` feature; descriptions, capabilities and effects work without it. The core's no-default-features normal dependency graph remains free of serde.

## HTTP, Console and Agent consumers

TypeScript `describeAction(action)`, `Module.describe()`, `Catalog.describeActions()` and `describeActions(modules)` read declarations without executing Actions. Rust provides `describe_action`, `Module::describe_actions` and `Catalog::describe_actions`. Returned descriptions own their data. Console bindings capture metadata from the actual installed Action, including across Rust's erased bindings.

Module JSON keeps the existing `actions: string[]` field. Only annotated modules add `metadata`, and only annotated Actions add entries to `actionMetadata`. Unannotated Modules retain their previous serialized shape. No executable handle, policy, resource value or parser is serialized. Console rejects action metadata for IDs absent from the module's installed actions.

The reference HTTP servers expose these same Modules through their authorized Console manifest. `ConsoleClient.manifest()` preserves the optional fields; `ConsoleClient.describeActions()` applies the same SDK projection. A thin Agent consumer can use that description and call the existing authorized Action route. It does not need a second operation registry or a replacement executor. Discovery authentication remains the host's responsibility.

The acceptance fixture exercises such a consumer over real HTTP, then checks the resulting database write and call record. It does not add an SDK MCP server or establish LLM tool-selection quality. The independent CLI Agent/MCP adapter remains a separate product integration.

## Structured input failures

TypeScript `ActionError.toJSON()` and Rust `ActionError::public()` produce the shared public shape:

```json
{
  "code": "invalid_input",
  "message": "Invalid input",
  "issues": [{ "path": ["rows", 0, "id"], "code": "invalid_value" }]
}
```

An issue has a path of field names/array positions and one of `invalid_value`, `required`, `out_of_range`, `invalid_format`. Empty paths represent root errors or paths that cannot safely be disclosed. Legacy errors without issues still serialize as `{code,message}`. Other error classifications do not carry input issues. The Console client accepts both old and new payloads and preserves the same issues on its reconstructed ActionError.

Automatic parser/codec failures keep the fixed public message and `invalid_value` code. They exclude parser messages, received values, stacks and causes. Only path prefixes declared by inline schema `properties` and `items` are exposed. Dynamic map keys and unknown keys are excluded; `$ref`, composed schemas and unresolved paths stop traversal. No schema reference is fetched. Without a declared input schema, automatic failures have root paths. TypeScript collects at most 16 structural parser issues, including Zod issues; an opaque or throwing diagnostic getter falls back to a root issue while retaining the original cause. Rust uses serde's typed path information and reports its first decoding failure. Missing Rust fields can resolve to a parent/root path; the SDK does not infer a field from private error text.

Domain validation can deliberately attach public issues: the TypeScript ActionError constructor accepts `issues` beside `cause`, and Rust has `with_issues`. These explicit paths and application messages are author-owned public data, not a general data-loss filter. The original cause remains available through native `cause`/`Error::source()` and is absent from the public projection. Hosts should serialize the public projection when responding or logging external failures, rather than a native debug dump.

Both reference HTTP servers map malformed JSON to a root `invalid_format` issue and use the shared public projection for Action failures. Other framework-level transport errors remain host-owned. Output invariant failures remain internal and cannot expose input issues.

## Compatibility and evidence

The shared [description](../conformance/fixtures/action-metadata.json) and [input-path fixtures](../conformance/fixtures/input-issues.json) are consumed by both SDKs. Tests preserve legacy Module payloads, native non-serializable inputs/results, existing TS inference and Rust trait implementations, optional JSON, authorization, normalization, private causes and owned snapshots.

See the [acceptance record](sdk-metadata-acceptance.md) for commands, actual HTTP outcomes and tested platform limits.
