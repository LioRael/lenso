# Contributing to Lenso vNext

Lenso is a local-first runtime built from replaceable Plugins, typed
Capabilities, Runtime Drivers, and Execution Adapters. Read [`CONTEXT.md`](CONTEXT.md)
for the vocabulary and invariants before changing framework behavior.

Delta, AI tools, and any particular editor are optional. A contributor needs
only Git, a fork, and the tools required by the part they change.

## Propose a contribution

Use a GitHub Issue as the durable handoff when you do not have upstream write
access. Include:

- the problem or change summary;
- the fork URL and branch;
- the immutable full commit SHA;
- focused checks that passed; and
- known limitations or checks that were not run.

The maintainer reviews the pinned revision, not a moving branch. If a fork is
impractical, publish a `git format-patch` file at a durable accessible URL and
include its source SHA in the Issue. Issues are not arbitrary patch-upload
storage.

## Minimal fork workflow

```sh
git clone https://github.com/<you>/lenso.git
cd lenso
git remote add upstream https://github.com/LioRael/lenso.git
git fetch upstream main
git switch -c <topic> upstream/main
# edit with any editor, then run focused checks
git add <changed-files>
git commit -m "type(scope): describe the change"
git push -u origin <topic>
```

Open an Issue at <https://github.com/LioRael/lenso/issues/new> with the fork
branch URL, full commit SHA, validation, and limitations. A patch alternative
is:

```sh
git format-patch --stdout upstream/main..<topic> > /tmp/lenso-<topic>.patch
```

Publish that file somewhere durable and link it from the Issue.

## Validation

Choose the smallest check that exercises the changed behavior:

- prose or documentation: check links, examples, and formatting;
- Rust code: run the affected package tests and checks;
- workflow, executable script, Land skill, or build configuration: run focused
  syntax/configuration checks and the relevant script tests;
- unknown or cross-cutting changes: use the repository's broader gate.

Contributors do not need to run every platform or release check. The upstream
candidate gate supplies the repository's native and portable WebAssembly proof
when the final change requires it.

## Maintainer integration

The maintainer imports the immutable Issue revision into an isolated checkout,
reviews untrusted workflow and script changes before using upstream
credentials, integrates it on the current `origin/main`, runs one final
candidate check, and lands that exact SHA with a normal fast-forward. Fork CI
is useful context but does not replace the upstream candidate result. The
maintainer preserves contributor authorship and links the Issue and final
commit.

Delta and agent paths are optional:

- **Delta Land Changes** opens a dedicated Land subthread.
- **`/land`** is an optional agent invocation in the current task, not a
  general shell command and not a GitHub permission grant.
- Other agents may explicitly read
  [`.agents/skills/land/SKILL.md`](.agents/skills/land/SKILL.md) or use their
  supported skill mechanism. The skill does not grant write access.
- Plain Git maintainers can follow the short path below without an agent.

## Maintainer Git landing

```sh
git fetch origin main
git switch -c land/<topic> origin/main
# import and review the contributor's immutable SHA
git push origin HEAD:refs/heads/delta/verify/<task>/<attempt>
gh run list --repo LioRael/lenso --workflow ci.yml \
  --branch delta/verify/<task>/<attempt>
gh run view <run-id> --repo LioRael/lenso \
  --json workflowName,event,headBranch,headSha,jobs,url
git fetch origin main
git push origin <candidate-sha>:refs/heads/main
gh api repos/LioRael/lenso/git/ref/heads/main --jq .object.sha
```

Accept only a successful `quality` job for the exact candidate SHA and
candidate push attempt. If `main` advances, integrate and validate again; if a
candidate is already reachable from the remote tip, keep its SHA unchanged.
Use normal pushes only. Publication, tags, and deployment are separate
authorized operations.

## Scope and commits

Do not restore v0.3.x Service, Provider, System Plane, Console, Story, Auth,
PostgreSQL, migration, or TypeScript Service Kit code to this branch. If a
feature needs one of those concepts, express it first as a vNext Capability,
ordinary Plugin, Execution Adapter, authoring tool, or separate repository.

Use Conventional Commits:

```text
<type>[optional scope]: <imperative summary>
```

Stage only files belonging to the requested change. Keep generated lockfile
changes with the manifest change that caused them.
