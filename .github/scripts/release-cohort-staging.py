#!/usr/bin/env python3
"""Keep published dependencies external and validate scratch-only graph pruning."""

import copy
import json
from pathlib import Path
import re
import sys
import tomllib


def identity(package):
    return package["name"], package["version"], package.get("source")


def packages(lock):
    result = {identity(p): p for p in lock["package"]}
    if len(result) != len(lock["package"]):
        raise ValueError("duplicate locked package identity")
    return result


def merge_prefetch(root):
    """Combine immutable registry inputs, not unrelated resolution graphs."""
    records = {}
    declarations = {}
    for directory in sorted(root.glob("*/")):
        metadata_file = directory / "metadata.json"
        if not metadata_file.is_file():
            continue
        metadata = json.loads(metadata_file.read_text())
        lock = packages(tomllib.loads((directory / "Cargo.lock").read_text()))
        for package in metadata["packages"]:
            key = identity(package)
            if key[2] != "registry+https://github.com/rust-lang/crates.io-index":
                continue
            if key not in lock or not re.fullmatch(r"[0-9a-f]{64}", lock[key].get("checksum", "")):
                raise ValueError(f"missing exact registry checksum: {key}")
            record = {field: lock[key][field] for field in ("name", "version", "source", "checksum")}
            if key in records and (records[key] != record or declarations[key] != package):
                raise ValueError(f"inconsistent prefetched registry identity: {key}")
            records[key] = record
            declarations[key] = package
    lines = ["# Registry checksum catalog; independent fetch graphs are not combined.", "version = 4", ""]
    for key in sorted(records):
        lines.append("[[package]]")
        lines.extend(f"{field} = {json.dumps(value)}" for field, value in records[key].items())
        lines.append("")
    (root / "Cargo.lock").write_text("\n".join(lines))
    return {"packages": list(declarations.values())}


def dependency_identity(value, graph):
    parts = value.split(" ", 2)
    matches = [key for key in graph if key[0] == parts[0]
               and (len(parts) < 2 or key[1] == parts[1])
               and (len(parts) < 3 or key[2] == parts[2].strip("()"))]
    if len(matches) != 1:
        raise ValueError(f"ambiguous or missing locked dependency: {value}")
    return matches[0]


def stage(root, metadata, external):
    """Change only the scratch workspace root, never published manifests."""
    manifest = root / "Cargo.toml"
    original = manifest.read_text()
    document = tomllib.loads(original)
    members = {p["id"]: p for p in metadata["packages"]
               if p["id"] in metadata["workspace_members"]}
    retained = [p for p in members.values() if p["name"] not in external]
    paths = lambda ps: [str(Path(p["manifest_path"]).parent.relative_to(root)) for p in ps]
    defaults = [p for p in retained if p["id"] in metadata["workspace_default_members"]]
    excluded = document["workspace"].get("exclude", []) + paths(
        p for p in members.values() if p["name"] in external)
    match = re.search(r"(?m)^\[workspace\]\s*\n", original)
    if not match:
        raise ValueError("missing explicit workspace table")
    end = re.search(r"(?m)^\[", original[match.end():])
    stop = match.end() + end.start() if end else len(original)
    section = original[match.end():stop]
    for field, values in [("members", paths(retained)),
                          ("default-members", paths(defaults)), ("exclude", excluded)]:
        pattern = rf"(?ms)^{field}\s*=\s*\[.*?\]"
        replacement = f"{field} = {json.dumps(values)}"
        if re.search(pattern, section):
            section = re.sub(pattern, lambda _: replacement, section, count=1)
        else:
            section = replacement + "\n" + section
    updated = original[:match.end()] + section + original[stop:]
    parsed = tomllib.loads(updated)
    for field in ("members", "default-members", "exclude"):
        document["workspace"][field] = parsed["workspace"][field]
    if document != parsed:
        raise ValueError("staging changed more than workspace membership")
    manifest.write_text(updated)


def validate(before, after, metadata, external, fixtures, resolved=None):
    """Allow only dev-edge removal and its resulting unreachable package pruning.

    All surviving versions, sources, checksums, fields and active runtime edges
    remain identical. External roots also cease locking their inactive optional
    features; their registry dependencies can lose optional features too.
    Such edges require both a declaration and independent resolved metadata
    proving they are inactive. Cargo may mark an unreachable patch as unused.
    """
    old = packages(before)
    expected = copy.deepcopy(before)
    graph = packages(expected)
    declarations = {identity(p): p for p in metadata["packages"]}
    active = {}
    if resolved:
        identities = {p["id"]: identity(p) for p in resolved["packages"]}
        active = {identities[node["id"]]: {identities[d["pkg"]][0] for d in node["deps"]}
                  for node in resolved["resolve"]["nodes"]}
    actual = packages(after)
    for key, package in graph.items():
        removable = {dependency for owner, dependency in fixtures
                     if owner == key[0] and key[2] is None}
        external_path = key[0] in external and key[2] is None
        registry_dependency = key[2] == "registry+https://github.com/rust-lang/crates.io-index"
        if (external_path or registry_dependency) and key in declarations:
            declared = declarations[key]["dependencies"]
            dev = {d["name"] for d in declared if d["kind"] == "dev"}
            production = {d["name"] for d in declared if d["kind"] != "dev"}
            required = {d["name"] for d in declared if d["kind"] != "dev" and not d["optional"]}
            if external_path:
                removable |= dev - production
            remaining = {dependency_identity(value, actual)[0]
                         for value in actual.get(key, {}).get("dependencies", [])}
            removable |= {d["name"] for d in declared if d["kind"] != "dev"
                          and d["optional"] and d["name"] not in required and resolved and key in active
                          and d["name"] not in active[key] and d["name"] not in remaining}
        if "dependencies" in package:
            package["dependencies"] = [value for value in package["dependencies"]
                                       if dependency_identity(value, old)[0] not in removable]
            if not package["dependencies"]:
                del package["dependencies"]
    roots = {identity(p) for p in metadata["packages"]
             if p["id"] in metadata["workspace_members"] and p["name"] not in external}

    def reachable_from(start, edges):
        reachable = set()
        pending = list(start)
        while pending:
            key = pending.pop()
            if key in reachable:
                continue
            reachable.add(key)
            pending.extend(dependency_identity(value, old)
                           for value in edges[key].get("dependencies", []))
        return reachable

    original_roots = {identity(p) for p in metadata["packages"]
                      if p["id"] in metadata["workspace_members"] and identity(p) in old}
    removed = reachable_from(original_roots, old) - reachable_from(roots, graph)
    expected["package"] = [p for p in expected["package"] if identity(p) not in removed]
    unused = expected.get("patch", {}).get("unused", [])
    unused += [{"name": key[0], "version": key[1]} for key in removed
               if key[0] in external and key[2] is None]
    if unused:
        expected.setdefault("patch", {})["unused"] = unused

    def canonical(lock):
        result = copy.deepcopy(lock)
        lookup = packages(result)
        for p in result["package"]:
            if "dependencies" in p:
                p["dependencies"] = sorted(
                    (dependency_identity(v, lookup) for v in p["dependencies"]), key=str)
        result["package"] = sorted(result["package"], key=lambda p: str(identity(p)))
        if "unused" in result.get("patch", {}):
            result["patch"]["unused"] = sorted(result["patch"]["unused"], key=str)
        return result

    if canonical(expected) != canonical(after):
        raise ValueError("scratch lock drift exceeds external test-root/unreachable-package pruning")


def validate_registry_conversion(before, after, prefetched, cohort):
    """Clean-room source substitution cannot upgrade packages or reroute edges."""
    old, actual, fetched = packages(before), packages(after), packages(prefetched)
    planned = {(p["package_name"], p["version"], None) for p in cohort}
    mapping = {}
    for key, package in actual.items():
        previous = key if key in old else (key[0], key[1], None)
        if previous not in old:
            raise ValueError(f"unlocked clean-room package identity: {key}")
        expected = copy.deepcopy(old[previous])
        if key[2] is None:
            if key not in planned:
                raise ValueError(f"unexpected clean-room path source: {key}")
        elif previous != key:
            if key[2] != "registry+https://github.com/rust-lang/crates.io-index" or key not in fetched:
                raise ValueError(f"unlocked clean-room registry substitution: {key}")
            expected["source"] = key[2]
            expected["checksum"] = fetched[key]["checksum"]
        mapping[previous] = key
        if {k: v for k, v in expected.items() if k != "dependencies"} != {
                k: v for k, v in package.items() if k != "dependencies"}:
            raise ValueError(f"clean-room identity/checksum drift: {key}")
    if not planned <= actual.keys():
        raise ValueError("missing clean-room cohort identity")
    for previous, key in mapping.items():
        expected_edges = {mapping.get(dependency_identity(v, old), dependency_identity(v, old))
                          for v in old[previous].get("dependencies", [])}
        actual_edges = {dependency_identity(v, actual) for v in actual[key].get("dependencies", [])}
        if expected_edges != actual_edges:
            raise ValueError(f"clean-room dependency-edge drift: {key}")


def project_clean_room(before, prefetched, cohort):
    """Project locked edges to exact registry identities without re-resolution."""
    old, fetched = packages(before), packages(prefetched)
    planned = {(p["package_name"], p["version"], None) for p in cohort}
    pending = list(planned)
    retained = {}
    mapping = {}
    while pending:
        key = pending.pop()
        if key in retained:
            continue
        package = copy.deepcopy(old[key])
        destination = key
        if key[2] is None and key not in planned:
            destination = key[0], key[1], "registry+https://github.com/rust-lang/crates.io-index"
            if destination not in fetched:
                raise ValueError(f"missing exact prefetched registry identity: {destination}")
            package["source"] = destination[2]
            package["checksum"] = fetched[destination]["checksum"]
        mapping[key] = destination
        retained[key] = package
        pending.extend(dependency_identity(v, old) for v in package.get("dependencies", []))
    names = [key[0] for key in mapping.values()]
    versions = [(key[0], key[1]) for key in mapping.values()]

    def reference(key):
        value = key[0]
        if names.count(key[0]) > 1:
            value += " " + key[1]
            if versions.count((key[0], key[1])) > 1:
                if key[2] is None:
                    raise ValueError(f"ambiguous path identity: {key}")
                value += " (" + key[2] + ")"
        return value

    for package in retained.values():
        if "dependencies" in package:
            package["dependencies"] = sorted(reference(mapping[dependency_identity(v, old)])
                                             for v in package["dependencies"])
    result = {"version": before["version"], "package": list(retained.values())}
    validate_registry_conversion(before, result, prefetched, cohort)
    lines = ["# Exact clean-room projection of the validated staging lock.",
             f'version = {result["version"]}', ""]
    for package in sorted(result["package"], key=lambda p: str(identity(p))):
        lines.append("[[package]]")
        for field in ("name", "version", "source", "checksum"):
            if field in package:
                lines.append(f"{field} = {json.dumps(package[field])}")
        if package.get("dependencies"):
            lines += ["dependencies = [", *[f" {json.dumps(v)}," for v in package["dependencies"]], "]"]
        lines.append("")
    return "\n".join(lines)


def main():
    mode, *args = sys.argv[1:]
    if mode == "stage":
        root, metadata, external = args
        stage(Path(root), json.loads(Path(metadata).read_text()), set(json.loads(external)))
    elif mode == "validate":
        before, after, metadata, external, fixtures, resolved = args
        validate(tomllib.loads(Path(before).read_text()), tomllib.loads(Path(after).read_text()),
                 json.loads(Path(metadata).read_text()), set(json.loads(external)),
                 [line.split("\t") for line in Path(fixtures).read_text().splitlines()],
                 json.loads(Path(resolved).read_text()))
    elif mode == "registry-conversion":
        before, after, prefetched, cohort = args
        validate_registry_conversion(
            tomllib.loads(Path(before).read_text()), tomllib.loads(Path(after).read_text()),
            tomllib.loads(Path(prefetched).read_text()), json.loads(cohort))
    elif mode == "clean-room":
        before, prefetched, cohort, destination = args
        Path(destination).write_text(project_clean_room(
            tomllib.loads(Path(before).read_text()), tomllib.loads(Path(prefetched).read_text()),
            json.loads(cohort)))
    elif mode == "optional-pruning":
        before, after, resolved = args
        metadata = json.loads(Path(resolved).read_text())
        roots = {p["name"] for p in metadata["packages"] if p["id"] in metadata["workspace_members"]}
        external = {p["name"] for p in metadata["packages"] if p["name"] not in roots}
        validate(tomllib.loads(Path(before).read_text()), tomllib.loads(Path(after).read_text()),
                 metadata, external, [], metadata)
    elif mode == "merge-prefetch":
        print(json.dumps(merge_prefetch(Path(args[0]))))
    else:
        raise ValueError(f"unknown operation: {mode}")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError) as error:
        sys.exit(str(error))
