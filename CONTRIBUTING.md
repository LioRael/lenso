# Contributing to lenso-cli

Contributions are welcome. Delta, AI assistance, and editor integrations are optional; ordinary Git and GitHub remain supported. Maintainers review every change before it receives repository credentials or is landed.

## Choose a handoff

### GitHub Issue (recommended for a fork)

1. Fork this repository and create a focused branch.
2. Run the narrow checks that cover your change (for example, `cargo fmt --all -- --check`, a focused `cargo test --locked -p lenso-cli`, or a workflow YAML parse check). Record limitations and checks you could not run.
3. Open a GitHub Issue at <https://github.com/LioRael/lenso-cli/issues> with:
   - a short summary and motivation;
   - your fork URL and branch;
   - the immutable full commit SHA to review;
   - focused validation commands and results;
   - known limitations or remaining risks.

An Issue is a handoff, not an attachment mechanism: do not claim that a patch is attached to it. A maintainer can inspect the fork and import the commit after reviewing untrusted workflow and script changes.

### Durable patch handoff

When a fork or Issue is not suitable, create a durable format-patch bundle and share it through your normal approved channel:

```sh
git format-patch --binary --full-index origin/main..HEAD --output-directory /tmp/lenso-cli-patches
tar -czf /tmp/lenso-cli-patches.tgz -C /tmp lenso-cli-patches
```

Include the same summary, full SHA, validation, and limitations. A maintainer imports the patch into a clean checkout and reviews it as untrusted input.

## Review and landing

Maintainers import and inspect workflow and executable-script changes before granting credentials. The normal landing sequence is candidate-first: focused local checks, a meaningful review, one candidate CI run on a `delta/verify/**` ref, then a normal fast-forward of the exact verified SHA to `main` only while the destination is unchanged. Candidate CI is the authoritative full repository proof; local checks are not a substitute.

Delta users may use **Delta Land Changes** or `/land` when the task is explicitly authorized for this repository. Other agents can prepare a reviewed commit and hand it off. Plain Git users should use normal branches, fetches, and fast-forward pushes. `/land` is not a universal shell command and does not grant repository or credential permissions.

See the repository [README](README.md) for the project workflow and [the maintainer landing skill](.agents/skills/land/SKILL.md) for the candidate procedure.
