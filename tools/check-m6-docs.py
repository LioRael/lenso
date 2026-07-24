#!/usr/bin/env python3
"""Check versioned M6 runbooks against the generated GA Support Manifest."""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOCS = [
    ROOT / "docs/operations/m6/upgrade-and-contracts.md",
    ROOT / "docs/operations/m6/failure-backup-and-disaster.md",
    ROOT / "docs/operations/m6/security-and-release.md",
    ROOT / "docs/operations/m6/incident-map.md",
    ROOT / "docs/security/m6-threat-model.md",
]
REQUIRED_COMMANDS = [
    "lenso ga support-check",
    "lenso ga service-upgrade",
    "lenso ga contract-retire",
]
REQUIRED_BOUNDARIES = [
    "contract_retirement",
    "single_region_disaster_cutover",
    "single_region_disaster_failback",
    "production",
]


def main() -> None:
    texts = [path.read_text() for path in DOCS]
    joined = "\n".join(texts)
    digest = f"sha256:{hashlib.sha256(joined.encode()).hexdigest()}"
    manifest = json.loads(
        (ROOT / "contracts/ga/lenso.ga-support-manifest.v1.json").read_text()
    )
    if manifest["documentation"] != {"version": "m6-ga", "digest": digest}:
        raise SystemExit("GA Support Manifest documentation identity is stale")
    for command in REQUIRED_COMMANDS:
        if command not in joined:
            raise SystemExit(f"missing public runbook command: {command}")
    for boundary in REQUIRED_BOUNDARIES:
        if boundary not in joined:
            raise SystemExit(f"missing runbook Approval Boundary: {boundary}")
    if "long-lived" not in joined or "Provider" not in joined or "Autonomous Service" not in joined:
        raise SystemExit("runbooks omit release credential or Provider/Autonomous boundaries")
    check_local_links()
    check_contract_references(joined)
    check_support_claims(manifest)
    if not re.search(r"\b[a-z][a-z0-9_]+_(?:invalid|blocked|failed|stale|missing)\b", joined):
        raise SystemExit("runbooks omit stable machine-readable issue codes")
    print(f"M6 documentation acceptance passed: {len(DOCS)} files, {digest}")


def check_local_links() -> None:
    pattern = re.compile(r"\[[^\]]+\]\(([^)]+)\)")
    for document in DOCS:
        for target in pattern.findall(document.read_text()):
            target = target.split("#", 1)[0]
            if not target or "://" in target or target.startswith("mailto:"):
                continue
            resolved = (document.parent / target).resolve()
            if not resolved.exists():
                raise SystemExit(f"{document.relative_to(ROOT)}: broken local link {target}")


def check_contract_references(joined: str) -> None:
    protocols = set(re.findall(r"`(lenso\.[a-z0-9.-]+\.v[0-9]+)`", joined))
    contract_text = "\n".join(
        path.read_text(errors="ignore")
        for path in (ROOT / "contracts").rglob("*")
        if path.is_file()
    )
    missing = sorted(protocol for protocol in protocols if protocol not in contract_text)
    if missing:
        raise SystemExit(f"runbooks reference unknown contract protocols: {missing}")


def check_support_claims(manifest: dict[str, object]) -> None:
    guidance = (ROOT / "docs/operations/ga-support.md").read_text()
    expected = {
        f"{component['componentId']}@{component['version']}"
        for component in manifest["components"]
    }
    expected.update(item["version"] for item in manifest["manifestFormats"])
    expected.update(manifest["stateVersions"])
    missing = sorted(claim for claim in expected if claim not in guidance)
    if missing:
        raise SystemExit(f"GA guidance omits exact support claims: {missing}")


if __name__ == "__main__":
    main()
