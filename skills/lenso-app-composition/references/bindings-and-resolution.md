# Bindings and resolution

Every Module requirement declares a Capability identity, exact Descriptor
version, and one cardinality. App Composition selects providers by stable
App-local Instance key.

## Cardinality

| Cardinality | Valid bindings | Consumer receives | Failure |
| --- | --- | --- | --- |
| `one` | exactly one | one typed handle | missing or more than one provider |
| `optional` | zero or one | `None` or one typed handle | more than one provider; a selected broken provider still fails boot |
| `many` | zero or more explicit providers | handles in deterministic resolved order | duplicate/incompatible binding |

`optional` describes absence of a binding, not tolerance for a declared
provider that cannot prepare. Required request/stream edges determine activation
order and cannot form a cycle. Event and observation edges are validated but do
not imply the same activation dependency.

## `many` example

```json
{
  "key": "web-ingress",
  "package": "lenso.web-ingress",
  "requires": [
    {
      "capability_id": "lenso.http.endpoint@1",
      "descriptor_version": "1.0.0",
      "cardinality": "many"
    }
  ]
}
```

```json
[
  {
    "consumer": "web-ingress",
    "capability_id": "lenso.http.endpoint@1",
    "descriptor_version": "1.0.0",
    "provider": "orders-http"
  },
  {
    "consumer": "web-ingress",
    "capability_id": "lenso.http.endpoint@1",
    "descriptor_version": "1.0.0",
    "provider": "status-http"
  }
]
```

Do not infer bindings from package dependencies, linked factories, matching
names, or a global registry. A package can be linked but unselected, and the
same package can appear under several Instance keys.

## Check before resolve

`check` must fail closed for at least these classes:

- missing package input or lock mismatch;
- unavailable execution class or missing Bun entrypoint;
- non-empty configuration without a valid Schema, raw sensitive value, or
  malformed secret reference;
- missing/stale Descriptor and generated artifacts;
- endpoint Operation/kind/version mismatch;
- missing, ambiguous, duplicate, incompatible, or cyclic binding; and
- missing/invalid Web role or lane referenced by a selected recipe.

`resolve` materializes the validated graph, exact package revisions, canonical
configuration bytes, endpoint and binding tables, execution policy, and lanes
into `ResolvedAppPlan`. Review the canonical bytes or diff. Loading a modified,
malformed, invalid, or non-canonical Plan must fail; Kernel never re-resolves it.

## Removal check

To remove an optional Module, delete its package input, Instance, provided
bindings, configuration, and unused contract inputs. Rebind or remove any
consumer that declared it as required. Re-run package-manager lock, `check`, and
`resolve`; the remaining Plan must not contain an orphan endpoint, binding,
execution class, or secret reference.
