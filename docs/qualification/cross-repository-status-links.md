# Cross-repository qualification links

The [qualification ledger](qualification-status.json) is the sole current
status authority for Lenso source owners. A Runtime, Web, Auth, Console, or
Agent README may describe its API, ownership boundary, and how to reproduce a
check. It must not independently decide that a feature is supported, released,
target-qualified, or production-qualified.

## Minimal README anchor

After this ledger has a remotely resolvable canonical revision, every affected
source-owner README should contain one short link equivalent to:

> This README describes source behavior, not current maturity. The canonical
> implementation, release, and Environment-plus-Infrastructure qualification
> evidence is the [Lenso qualification ledger](https://github.com/LioRael/lenso/blob/main/docs/qualification/qualification-status.json).

The moving `main` URL is intentional: it names the current authority. A ledger
record, not this prose link, carries the immutable source revision, retained
receipt, and exact qualification tuple for a specific assertion.

Before the ledger itself is remotely resolvable, do not add a link to a guessed
or unavailable revision. Keep the README free of maturity verdicts and retain
the local candidate's cohort ID, source revision, and snapshot digest with the
delivery evidence instead.

## What stays local to an owner README

- API compatibility, intentional unsupported behavior, and safety boundaries.
- Reproduction commands, provided they say what they do *not* qualify.
- Links to a design decision or a source-owned raw receipt.

The README must link to the ledger rather than copy a green test count, target
name, release version, or an unscoped word such as “supported” into a maturity
claim. An owner may cite its own receipt only as input to a matching ledger
record.

## Initial migration audit

The audited runtime, Web, and Auth READMEs all need this anchor before they can
be treated as status-neutral:

| Owner | Existing drift | Minimal correction |
| --- | --- | --- |
| `lenso-runtime-rust` | `README.md`'s “Host support policy” declares Native production support and Browser target support. | Replace maturity verdicts with API/host-boundary text and the single ledger link. Keep the explicit fallback limitation. |
| `lenso-web` | `README.md` embeds release workflow and event-target qualification language without a canonical status link. | Keep transport/API facts; move release and qualification verdicts to the ledger and add the link. |
| `lenso-auth-plugin` | `README.md` calls out OAuth composition “target qualification gates” but does not point to the canonical record. | Keep the composition boundary and limitations; add the link and put any actual local/target verdict in a ledger record. |

This migration does not erase historical evidence documents. It only prevents a
source README from becoming a second mutable qualification board.
