# Agents

This directory documents how coding agents and maintainers navigate the Lenso
repository. It does not define the product-level Agent Module or Agent Harness;
those remain optional Lenso product Modules governed by the architecture and
their owning repositories.

| Document | Purpose |
| --- | --- |
| [`domain.md`](domain.md) | Load the vNext vocabulary and architecture authority before changing code. |
| [`issue-tracker.md`](issue-tracker.md) | Work with GitHub issues and the vNext delivery lane. |
| [`triage-labels.md`](triage-labels.md) | Translate canonical agent-triage roles into repository labels. |
| [`skills.md`](skills.md) | Discover, invoke, install, test, and maintain the canonical Lenso skill pack. |

## Authority order

Use repository guidance in this order:

1. Root [`AGENTS.md`](../../AGENTS.md) defines repository workflow and safety.
2. [`CONTEXT.md`](../../CONTEXT.md) and accepted ADRs define architecture and
   ownership.
3. The [`skills/`](../../skills/) pack turns a request into one executable
   workflow and names cross-repository handoffs.
4. Selected package versions, source, manifests, `--help`, and CI define the
   exact API and command surface.

Skills route work through these authorities. Installing a skill does not move
implementation ownership, grant production authority, or override repository
instructions.
