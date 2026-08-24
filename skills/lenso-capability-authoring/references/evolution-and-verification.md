# Evolution and verification

## Evolve deliberately

- Patch releases preserve the contract shape.
- Additive compatible changes advance the Descriptor minor version only when
  existing consumers and providers remain valid.
- Breaking role or shape changes create a new major identity.
- Module package releases do not silently change the Capability identity.

Run the installed generator's current compatibility workflow against the
previous accepted Descriptor. With the current CLI the shape is:

```sh
lenso-contract-codegen lint previous/capability.json capability.json
```

Record the intentional version decision beside the contract change rather than
inferring it from generated diffs. A patch with observable shape/meaning change
must fail; additive minor changes remain valid for existing consumers and
providers; removal, rename, narrowing, interaction change, or semantic reuse
creates a new `@major` series.

## Prove the contract

Require evidence for every changed Operation:

- Descriptor and Schema validation;
- deterministic generation and a freshness check for every declared artifact;
- typed consumer and provider compilation;
- success, domain-error, and runtime-failure preservation;
- cross-language wire vectors for portable contracts; and
- stream or event terminal, cancellation, backpressure, and partial-admission
  behavior when those interaction kinds are present.

Also compile/typecheck each generated Provider and Client from a clean checkout
of the package that distributes that language projection.
Exercise at least one old consumer or provider against an additive minor change
when compatibility is claimed. Verify that an older generated client preserves
an unknown Domain Error code and payload instead of discarding it.

The proof is complete when changing the Descriptor without regenerating makes
the freshness gate fail and at least one consumer-provider path exercises the
new contract.
