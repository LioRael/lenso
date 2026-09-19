---
name: lenso-land
description: Land reviewed Lenso core changes through candidate CI and a direct fast-forward to main.
metadata:
  delta-action: land
---

# Land Lenso core changes

This repository uses PR-free delivery. The destination is `origin/main`, and
the required candidate gate is the `quality` job in `.github/workflows/ci.yml`.
The workflow runs on `main` and on `delta/verify/**` push refs. Landing this
pilot does not publish packages or deploy a product; `release-plz` remains
paused.

## Prepare

1. Read the repository rules, current diff, staged files, and remotes. Keep
   unrelated dirty work out of the task-owned tree.
2. Obtain a Delta Review for the final intended diff. Pull reviewer changes
   before preparing the final commit; a verdict alone does not incorporate
   edits. Resolve mechanical conflicts and return unresolved product or
   migration intent to the owner.
3. Fetch the destination and record its full base SHA:

   ```sh
   git fetch origin main
   git rev-parse origin/main
   ```

   Finish all review fixes, rebasing, and squashing before candidate CI. Keep
   the final tree clean and use the repository's Conventional Commit policy.
4. Run the local checks required by `AGENTS.md`:

   ```sh
   cargo fmt --all -- --check
   cargo check --locked --workspace --all-targets
   cargo test --locked --workspace
   ```

   Use the shared `lenso-cargo` wrapper when it is available in the local
   framework checkout.

## Verify the exact candidate

1. Choose a unique task-owned ref such as
   `delta/verify/lenso-core-pilot/1`. Record the full candidate SHA and its
   base SHA, then push that commit without rewriting the ref:

   ```sh
   git push origin <candidate-sha>:refs/heads/delta/verify/<task>/<attempt>
   ```

2. Wait for the `CI` workflow created by that `push`. Inspect the run through
   GitHub CLI and match all of these values before accepting it:

   - repository `LioRael/lenso`;
   - workflow identity `CI` from `.github/workflows/ci.yml`;
   - event `push`;
   - candidate branch and exact `head_sha`;
   - one `quality` job with conclusion `success`, with no missing, skipped,
     cancelled, timed-out, or failed required job.

   A useful inspection sequence is:

   ```sh
   gh run list --repo LioRael/lenso --workflow ci.yml \
     --branch delta/verify/<task>/<attempt> --limit 10
   gh run view <run-id> --repo LioRael/lenso \
     --json workflowName,event,headBranch,headSha,status,conclusion,jobs,url
   ```

   The `quality` job is the proof for native checks and both
   `wasm32-unknown-unknown` and `wasm32-wasip2` checks. A local pass, a green
   run for another SHA, or a manually dispatched run for another ref is not
   candidate evidence.
3. Treat a failed or incomplete candidate as rejected. Keep its exact SHA and
   run URL in the review record, and use a fresh candidate attempt after the
   cause is fixed. Disposable red candidates are valid protection rehearsals;
   they are never promoted to `main`.

## Integrate

1. After the exact candidate passes, fetch `origin/main` again. If its SHA
   differs from the recorded base, integrate the change on the new base,
   obtain the resulting review, and run candidate CI again under a new
   candidate ref. The old run does not qualify the new commit.
2. If the destination is unchanged, promote the exact verified commit with a
   normal fast-forward push:

   ```sh
   git push origin <candidate-sha>:refs/heads/main
   ```

   A rejected push means the change has not landed. Fetch, integrate the
   competing update, and repeat validation; never force-push, amend, squash,
   or mint a synthetic status after CI.
3. Read the destination back and require the remote `main` SHA to equal the
   verified candidate SHA:

   ```sh
   gh api repos/LioRael/lenso/git/ref/heads/main --jq .object.sha
   ```

   Record the base SHA, candidate SHA, landed SHA, Delta Review link, exact
   workflow run URL and run attempt, required job result, local commands, and
   remote readback. Report landing, CI verification, publication, and
   deployment as separate outcomes.

## Protection checks

Use task-owned disposable refs to exercise the boundaries without changing
`main`: a deliberately failing candidate must produce a failed `quality` run
and remain unlanded, while an older candidate push after a different verified
commit has landed must receive a normal non-fast-forward rejection. If the
destination advances before the requested candidate, follow the integrate
path instead of reusing stale evidence.
