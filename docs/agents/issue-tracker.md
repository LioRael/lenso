# Issue tracker: GitHub

Issues and implementation specifications live in GitHub Issues. Use the `gh`
CLI for issue and pull-request operations.

## vNext delivery lane

Issue #577 is the vNext implementation specification. Its dependency-ordered
child tickets are #578 through #603.

- Create vNext worktrees from the latest `origin/main`.
- Target `main` in every vNext pull request.
- Treat `next` as a pre-cutover integration reference, not a delivery branch.
- Do not repurpose the vNext specification or reintroduce removed legacy
  implementation into `main`.
- Resolve blockers through normal issue and pull-request evidence before
  implementing a child ticket.

## Common commands

- Read an issue: `gh issue view <number> --comments`.
- List issues: `gh issue list --state open --json number,title,body,labels`.
- Create an issue: `gh issue create --title "..." --body "..."`.
- Comment: `gh issue comment <number> --body "..."`.
- Close: `gh issue close <number> --comment "..."`.

GitHub is the source of ticket history. The repository does not mirror ticket
bodies into a legacy implementation directory.
