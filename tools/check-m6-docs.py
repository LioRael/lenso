#!/usr/bin/env python3
"""Check versioned M6 runbooks against the generated GA Support Manifest."""

from __future__ import annotations

import hashlib
import json
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
    print(f"M6 documentation acceptance passed: {len(DOCS)} files, {digest}")


if __name__ == "__main__":
    main()
