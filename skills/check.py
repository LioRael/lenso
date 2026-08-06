#!/usr/bin/env python3
"""Deterministic public-skill acceptance for the M6 fresh-starter boundary."""

from __future__ import annotations

import json
import shutil
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CASES = ROOT / "skills" / "m6-acceptance-cases.json"
REQUIRED = {
    "lenso-autonomous-service-authoring": {
        "success", "unsupported_input", "safe_failure", "cleanup",
    },
    "lenso-api-client": {
        "context_preserved", "deadline_expired", "unsafe_retry", "cleanup",
    },
    "lenso-module-extraction": {
        "linked_preserved", "blocked_extraction", "approval_stop", "cleanup",
    },
    "lenso-contract-evolution": {
        "parallel_majors", "active_consumer_rejected", "retirement_stop", "cleanup",
    },
    "lenso-durable-workflow": {
        "versioned_definition", "identity_reuse_rejected",
        "undeclared_compensation_rejected", "cleanup",
    },
    "lenso-incident-recovery": {
        "process", "broker", "identity", "store", "coordination",
        "migration", "restore", "disaster",
    },
    "lenso-reviewed-release": {
        "changed_plan", "missing_coordinator", "receipt_recovery",
        "remote_mismatch", "production_stop",
    },
}
REQUIRED_TEXT = {
    "lenso-autonomous-service-authoring": ["GA Support Manifest", "Provider", "cleanup"],
    "lenso-api-client": ["Deadline", "Idempotency", "Call Policy"],
    "lenso-module-extraction": ["Extraction Plan", "Approval Boundary", "cleanup"],
    "lenso-contract-evolution": ["active", "parallel major", "Retirement"],
    "lenso-durable-workflow": ["compensation", "distributed transaction", "Approval Boundary"],
    "lenso-incident-recovery": ["continue", "fail-closed", "failback"],
    "lenso-reviewed-release": ["runbook", "shadow", "receipt", "production"],
}


def main() -> None:
    cases = json.loads(CASES.read_text())
    seen: dict[str, set[str]] = {name: set() for name in REQUIRED}
    for case in cases:
        skill = case["skill"]
        if skill not in REQUIRED:
            raise SystemExit(f"unknown M6 skill in fixture: {skill}")
        if case["humanPlan"] != case["agentPlan"]:
            raise SystemExit(f"{skill}/{case['scenario']}: human and agent plans diverge")
        if not case["issueCode"] or case["cleanupComplete"] is not True:
            raise SystemExit(f"{skill}/{case['scenario']}: unstable issue or cleanup evidence")
        seen[skill].add(case["scenario"])

    for skill, scenarios in REQUIRED.items():
        missing = scenarios - seen[skill]
        if missing:
            raise SystemExit(f"{skill}: missing scenarios {sorted(missing)}")
        skill_file = ROOT / "skills" / skill / "SKILL.md"
        agent_file = ROOT / "skills" / skill / "agents" / "openai.yaml"
        text = skill_file.read_text()
        if not text.startswith("---\n") or not agent_file.is_file():
            raise SystemExit(f"{skill}: incomplete public skill package")
        if "../" in text or "/Users/" in text or "target/debug" in text:
            raise SystemExit(f"{skill}: references a sibling checkout or internal build output")
        for phrase in REQUIRED_TEXT[skill]:
            if phrase.lower() not in text.lower():
                raise SystemExit(f"{skill}: missing required public behavior `{phrase}`")

    with tempfile.TemporaryDirectory(prefix="lenso-m6-skill-") as temporary:
        starter = Path(temporary) / "fresh-starter"
        starter.mkdir()
        for skill in REQUIRED:
            shutil.copytree(ROOT / "skills" / skill, starter / "skills" / skill)
        if str(starter).startswith(str(ROOT)):
            raise SystemExit("fresh-starter fixture remained inside the framework workspace")
    print(f"M6 public skill acceptance passed: {len(REQUIRED)} skills, {len(cases)} cases")


if __name__ == "__main__":
    main()
