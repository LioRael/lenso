---
name: lenso-land
description: Land reviewed Lenso changes through one candidate CI run and a normal fast-forward.
metadata:
  delta-action: land
---

# Land Lenso changes

This repository uses PR-free delivery. The destination is `origin/main`, and
the required candidate gate is the `quality` job in `.github/workflows/ci.yml`.
[`CONTRIBUTING.md`](../../../CONTRIBUTING.md) is the common human and maintainer
contract; this file is only the optional agent entry point.
When this skill is invoked in Delta, use the Delta-managed checkout directly;
do not create a nested Worktrunk worktree. A Land or delivery request
authorizes landing; review or skill installation alone does not.

## Prepare

1. Read `AGENTS.md`, status, diff, staged files, and remotes. Preserve
   unrelated work.
2. In a Delta run, obtain a Delta Review for the final diff and absorb reviewer
   edits before the final commit. Other maintainers may use an Issue review
   referring to the same immutable revision, as described in
   `CONTRIBUTING.md`.
3. Fetch `origin/main` and record its full SHA. Finish review fixes and
   rebasing before candidate CI.
4. Run only checks relevant to the changed files. Workflow, script, skill, and
   configuration changes use focused syntax/configuration checks; Rust changes
   retain their affected local checks. The candidate `quality` job remains the
   remote native/WASM proof.

## Verify one final candidate

1. Push the final commit once to a unique ref such as
   `delta/verify/lenso/<attempt>`:

   ```sh
   git push origin <candidate-sha>:refs/heads/delta/verify/<task>/<attempt>
   ```

2. Accept only the `CI` workflow run created by that `push` whose repository,
   workflow path/name, event, candidate ref, exact `head_sha`, run attempt, and
   `quality` job all match. `quality` must be completed and successful. Local
   checks, manual runs, and another SHA do not substitute for this evidence.
3. Record the candidate SHA, base SHA, review link (Delta Review for Delta
   runs; immutable-revision Issue review otherwise), run URL/attempt, and
   required job result.

## Integrate the same SHA

1. Fetch `origin/main` again. If it advanced and the candidate is not an
   ancestor of the remote tip, integrate on the new base, review, and create a
   new candidate SHA with a new CI run. If the candidate is already reachable,
   keep its SHA unchanged and record the current remote tip.
2. If the destination is unchanged, push the exact verified SHA normally:

   ```sh
   git push origin <candidate-sha>:refs/heads/main
   ```

   A rejected push has not landed. Fetch again; accept the candidate only when
   it is now reachable from remote `main`, otherwise repeat integration and
   candidate CI. Never force-push or rewrite verified commits.
3. Read back remote `main` and verify the candidate is an ancestor:

   ```sh
   landed_sha="$(gh api repos/LioRael/lenso/git/ref/heads/main --jq .object.sha)"
   git fetch origin main
   git merge-base --is-ancestor <candidate-sha> "$landed_sha"
   ```

   Report the landed SHA separately from CI, publication, and deployment.

## Release

Prepare versions and changelogs in a Delta thread with `release-plz update`,
review them, and land that exact source commit; do not create a release PR.
`.github/workflows/release-plz.yml` is dispatch-only and defaults to read-only
dry-run. Dispatch from `main` with the full `source_sha`, an exact
`release_set` JSON array, and `mode=dry-run`. Its read-only gate checks main
ancestry, the exact successful candidate `quality` run and attempt, the
`publish=true` allowlist, source versions, and a crates.io-derived unpublished
version set. Pinned release-plz dry-run is a no-upload check, not a proposed
release plan. `mode=publish` is separately authorized and retains the existing
action/tag/OIDC contract; this migration does not select it.

Protection failure/race rehearsals are migration qualification only and repeat
only when the mechanism changes. The pilot's failed candidate proved CI failure
and non-promotion, not server rejection of a failed SHA push.
