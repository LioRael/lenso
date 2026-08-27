#!/usr/bin/env python3
"""Validate the canonical Lenso skill pack and optional installed copies."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import deque
from pathlib import Path

EXPECTED_SKILLS = {
    "lenso-app-configuration",
    "lenso-business-planning",
    "lenso-capability-authoring",
    "lenso-plugin-authoring",
    "lenso-runtime-extension",
    "lenso-start",
}
LINK_PATTERN = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
JSON_BLOCK_PATTERN = re.compile(r"```json\n(.*?)\n```", re.DOTALL)
IGNORED_INSTALLED_FILES = {".DS_Store"}


def frontmatter(path: Path) -> dict[str, str]:
    lines = path.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0] != "---":
        raise ValueError("missing opening frontmatter delimiter")
    try:
        end = lines.index("---", 1)
    except ValueError as error:
        raise ValueError("missing closing frontmatter delimiter") from error
    values: dict[str, str] = {}
    for line in lines[1:end]:
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if ":" not in line:
            raise ValueError(f"invalid frontmatter line: {line}")
        key, value = line.split(":", 1)
        values[key.strip()] = value.strip().strip("\"'")
    return values


def local_links(path: Path) -> list[Path]:
    links: list[Path] = []
    for raw in LINK_PATTERN.findall(path.read_text(encoding="utf-8")):
        target = raw.strip().split("#", 1)[0]
        if not target or target.startswith(("http://", "https://", "mailto:")):
            continue
        links.append((path.parent / target).resolve())
    return links


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def payload_files(directory: Path) -> dict[Path, Path]:
    return {
        path.relative_to(directory): path
        for path in directory.rglob("*")
        if path.is_file()
        and path.name not in IGNORED_INSTALLED_FILES
        and path.suffix != ".pyc"
        and "__pycache__" not in path.parts
    }


def validate_pack(root: Path) -> list[str]:
    errors: list[str] = []
    skill_dirs = {path.parent.name: path.parent for path in root.glob("*/SKILL.md")}
    if set(skill_dirs) != EXPECTED_SKILLS:
        missing = sorted(EXPECTED_SKILLS - set(skill_dirs))
        extra = sorted(set(skill_dirs) - EXPECTED_SKILLS)
        if missing:
            errors.append(f"missing canonical skills: {', '.join(missing)}")
        if extra:
            errors.append(f"unexpected canonical skills: {', '.join(extra)}")

    for name, directory in sorted(skill_dirs.items()):
        entrypoint = directory / "SKILL.md"
        try:
            metadata = frontmatter(entrypoint)
        except ValueError as error:
            errors.append(f"{entrypoint.relative_to(root)}: {error}")
            continue
        if metadata.get("name") != name:
            errors.append(f"{name}: frontmatter name is {metadata.get('name')!r}")
        if not metadata.get("description"):
            errors.append(f"{name}: description is empty")

        openai = directory / "agents/openai.yaml"
        if not openai.is_file():
            errors.append(f"{name}: missing agents/openai.yaml")
        elif f"${name}" not in openai.read_text(encoding="utf-8"):
            errors.append(f"{name}: default prompt does not name ${name}")

        discovered: set[Path] = set()
        queue: deque[Path] = deque([entrypoint.resolve()])
        while queue:
            document = queue.popleft()
            if document in discovered:
                continue
            discovered.add(document)
            for target in local_links(document):
                if (
                    target.exists()
                    and target.suffix == ".md"
                    and directory.resolve() in target.parents
                ):
                    queue.append(target)

        references = {
            path.resolve() for path in (directory / "references").glob("*.md")
        }
        unreachable = sorted(references - discovered)
        for path in unreachable:
            errors.append(f"{path.relative_to(root)}: unreachable from SKILL.md")

    for document in root.rglob("*.md"):
        for target in local_links(document):
            if root.resolve() not in target.parents and target != root.resolve():
                errors.append(
                    f"{document.relative_to(root)}: local link leaves the skill pack: {target}"
                )
            elif not target.exists():
                errors.append(
                    f"{document.relative_to(root)}: broken local link to {target}"
                )
        for index, block in enumerate(
            JSON_BLOCK_PATTERN.findall(document.read_text(encoding="utf-8")), start=1
        ):
            try:
                json.loads(block)
            except json.JSONDecodeError as error:
                errors.append(
                    f"{document.relative_to(root)}: invalid JSON block {index}: {error}"
                )

    start = skill_dirs.get("lenso-start")
    if start is not None:
        openai_text = (start / "agents/openai.yaml").read_text(encoding="utf-8")
        if "allow_implicit_invocation: false" not in openai_text:
            errors.append("lenso-start: OpenAI policy must disable implicit invocation")

    return errors


def validate_installed(root: Path, installed_root: Path) -> list[str]:
    errors: list[str] = []
    for name in sorted(EXPECTED_SKILLS):
        canonical_directory = root / name
        installed_directory = installed_root / name
        canonical_files = payload_files(canonical_directory)
        installed_files = payload_files(installed_directory)
        for relative in sorted(canonical_files.keys() - installed_files.keys()):
            errors.append(f"installed copy missing: {installed_directory / relative}")
        for relative in sorted(installed_files.keys() - canonical_files.keys()):
            errors.append(
                f"installed copy has unexpected file: {installed_directory / relative}"
            )
        for relative in sorted(canonical_files.keys() & installed_files.keys()):
            if digest(canonical_files[relative]) != digest(installed_files[relative]):
                errors.append(
                    f"installed copy is stale: {installed_directory / relative}"
                )
    return errors


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--installed-root",
        type=Path,
        action="append",
        default=[],
        help="agent skills directory whose canonical Lenso copies must match",
    )
    return parser.parse_args()


def main() -> int:
    arguments = parse_args()
    root = Path(__file__).resolve().parents[1]
    errors = validate_pack(root)
    for installed_root in arguments.installed_root:
        errors.extend(validate_installed(root, installed_root.expanduser().resolve()))
    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return 1
    print(f"validated {len(EXPECTED_SKILLS)} canonical Lenso skills")
    for installed_root in arguments.installed_root:
        print(f"matched installed copy: {installed_root.expanduser().resolve()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
