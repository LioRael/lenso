# Host admission and target explanation

`lenso app explain --json` is the read-only explanation for the exact App
already materialized by a Host. It reads the Host's persisted admission output,
Plugin Root, resolved Plan, and generated bundle inventory; it does not choose a
replacement implementation, run a second dependency resolver, mutate the
Plugin Root, start a Host, or fall back to another target.

```sh
lenso app explain --root ./my-app --json
```

There is deliberately no `--profile` input. An execution-target capability
profile is emitted by the actual Runtime Driver/Adapter during Host build and
persisted with the selected implementation. Asking an operator to supply a
second JSON profile would make the CLI validate a claim rather than the Host
that will run the App.

## What it explains

The stable output schema is `lenso.app-explain.v1`. It has four independent
evidence groups:

- `target_capability_profiles`: the exact canonical profiles generated for the
  selected target implementations.
- `implementation_selection`: the persisted selected implementation and each
  genuine Runtime rejection, including such reasons as host-target mismatch,
  missing target capabilities, or runtime non-admission.
- `consumer_requirements`: each consumer's declared Capability requirement,
  the resolved provider binding, Host-authorized provider scope, and only the
  non-selected candidates whose reason can be established from Root or Plan
  evidence.
- `engine_execution`: execution/cache/Generation history when an owning Host
  has supplied it. A static App inspection explicitly reports that this evidence
  is unavailable; it never guesses why an Engine rebuild occurred.

The report is a projection of persisted facts. In particular, a provider that
is merely absent from a resolved binding is not mislabelled as an explicit
dependency-choice rejection, and a private Runtime choice is not reconstructed
from heuristics.

## Fail-closed target admission

Target admission happens while the Host is built or assembled, before an
incompatible implementation can become the selected runtime. For example, a
Plugin requiring a bidirectional Stream cannot be selected for a target whose
actual profile lacks `stream`; the build records the rejection or fails before
publishing a candidate Host. `app explain` then makes that decision inspectable.

An implementation can also declare target mechanics that are not inferable from
a Capability operation. The standard builders record `native-process` for Bun
and Process implementations, and `wasm-component` for a Wasm Component. A
Workers implementation must declare `workers`. These are immutable Bundle facts:
the Host combines them with Request/Stream/Event operation requirements and
fails closed. If an App includes an ordinary Bun candidate and a Workers-only
candidate, `app build` can select the former; `app explain --json` retains the
latter as a `missing_target_capabilities` rejection with a `workers`
requirement. It never upgrades a local Bun Host into a Workers target merely
because the alternate artifact exists.

The capability vocabulary and profile validation remain owned by
`lenso-process-protocol`; Runtime/Adapter packages generate the concrete
profiles. Application Capabilities remain Plan-bound. A private Driver resource
or infrastructure adapter does not become a second global resolver merely to
appear in this report.

## Boundaries

This command proves neither a release nor target qualification. It explains the
Host's selection and compatibility evidence. Real Workers, Browser, process,
storage, failure, lifecycle, and deployment qualification continue to be owned
by the applicable target and Environment-plus-Infrastructure cohort. Use
`lenso engine explain` for a bounded processing Session's cache/rebuild and
candidate-retention explanation; its `lenso.engine-explain.v1` output is
separate from this App admission report.
