# Agent instructions

Read [CONTRIBUTING.md](CONTRIBUTING.md) before preparing a contribution. Delta, AI, editor integrations, other agents, and plain Git are optional paths; do not assume `/land` is a shell command or a permission grant. Maintainers review untrusted workflow and executable-script changes before credentials. Candidate CI and a same-SHA normal fast-forward are required for landing; see [.agents/skills/land/SKILL.md](.agents/skills/land/SKILL.md).

Before planning or changing a release, read the repository-local
[`docs/release-process.md`](docs/release-process.md). Registry publication still
requires explicit approval; do not infer production authority from repository write access or publish outside the repository-local Trusted Publisher workflows. The current rollout keeps release workflows dispatch-only and read-only; no package, version, changelog, tag, release, or deployment operation is authorized by ordinary changes.

## Agent skills

### Issue tracker

Issues and PRDs are tracked in the central `LioRael/lenso` GitHub repository. See `docs/agents/issue-tracker.md`.

### Triage labels

Triage uses the five canonical labels in the central tracker. See `docs/agents/triage-labels.md`.

### Domain docs

Domain documentation uses a single-context layout. See `docs/agents/domain.md`.

## Local validation

Choose focused checks for the files changed. Do not repeat the entire workspace suite locally merely because a contribution is being prepared; candidate `quality` CI remains the full native, Windows, frontend, clean-room, and package proof. Preserve the CLI and architecture constraints documented in the repository and do not weaken release controls.
