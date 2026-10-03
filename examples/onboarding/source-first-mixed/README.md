# Rust HTTP, two TypeScript Instances, one App

This example composes a source Rust HTTP Plugin with two Instances of the same
source TypeScript Plugin. The right Instance calls the left through an optional
Capability dependency. Config files and provider choices live beside source.
The author supplies no factories, codecs, execution adapters or frozen packages.
The metadata Capability is reused from the existing canonical contract example;
its TypeScript projection is generated during the App build.

Use an exact lenso-js source checkout exposing `./authoring` and `./targets`.
The qualified checkout for this slice is `b6371aebfdf0b1a7056e0121ab6d0e7cb5206b73`.
Build its contract-runtime, process-protocol and bun-plugin packages, then:

```sh
node prepare-source-sdk.mjs /exact/lenso-js
lenso app discover --json
lenso app build --out dist
lenso app start --from dist
lenso app build --target workers --out dist-workers \
  --workers-runtime /exact/lenso-js/packages/lenso-workers-runtime \
  --wasm-bindgen /exact/wasm-bindgen-0.2.127
python3 verify.py --cli /exact/lenso --native dist --workers dist-workers \
  --workerd /exact/workerd --output verification.json
```

The helper declares and locks a real local source dependency. It does not pack,
install a Lenso Plugin, or pretend that an unpublished SDK feature exists in the
older registry version. Commit the resulting manifest and lock when adopting a
specific source dependency in your own project; this fixture keeps local paths
out of the repository.

`GET /instances` first returns `["left:1:hello","right:1:left:2:hello"]`.
A second Native request returns `["left:3:hello","right:2:left:4:hello"]`.
Workers creates a fresh App per HTTP event and repeats the first response.
The verifier compares the resolved logical graph after the explicit Bun-to-JS
execution lowering: only execution class/profile, target facilities and actual
executable artifact revision change. Instance IDs, configs, descriptors and
provider bindings remain identical. Executable bytes differ by target.

This Core Workers JS adapter currently supports Request. Stream/Event fail
before execution even when the SDK can package them; SDK Stream qualification
alone is not complete Kernel/App support. Other target-specific Rust Plugins
still need compatible target facilities. Native Rust changes require relinking.
Local config/source development uses `app build` or `app dev`; immutable bundles
and portable distributions retain their separate artifact boundary.
