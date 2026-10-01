#!/usr/bin/env python3
"""Exercise real Cargo dev-root semantics and strict scratch graph validation."""

import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import tomllib

spec = importlib.util.spec_from_file_location(
    "staging", Path(__file__).with_name("release-cohort-staging.py"))
staging = importlib.util.module_from_spec(spec)
spec.loader.exec_module(staging)


with tempfile.TemporaryDirectory(prefix="lenso-external-dev-root-") as directory:
    root = Path(directory).resolve()
    (root / "Cargo.toml").write_text('''[workspace]
members = ["planned", "published-bun", "runtime", "optional"]
resolver = "2"

[patch.crates-io]
runtime-dep = { path = "runtime" }
optional-dep = { path = "optional" }
''')
    manifests = {
        "planned": '''[package]
name = "cohort-native"
version = "0.3.20"
edition = "2024"
[dependencies]
published-bun = { path = "../published-bun", version = "=0.1.17" }
''',
        "published-bun": '''[package]
name = "published-bun"
version = "0.1.17"
edition = "2024"
[dependencies]
runtime-dep = { path = "../runtime", version = "=1.0.0" }
optional-dep = { path = "../optional", version = "=1.0.0", optional = true }
[dev-dependencies]
cohort-native = { path = "../planned", version = "=0.3.20" }
''',
        "runtime": '''[package]
name = "runtime-dep"
version = "1.0.0"
edition = "2024"
''',
        "optional": '''[package]
name = "optional-dep"
version = "1.0.0"
edition = "2024"
''',
    }
    for directory, manifest in manifests.items():
        crate = root / directory
        (crate / "src").mkdir(parents=True)
        (crate / "Cargo.toml").write_text(manifest)
        (crate / "src/lib.rs").write_text("")

    def metadata(*flags):
        return json.loads(subprocess.check_output(
            ["cargo", "metadata", "--offline", "--format-version", "1", *flags], cwd=root))

    original = metadata()
    before = tomllib.loads((root / "Cargo.lock").read_text())
    published = root / "published-bun/Cargo.toml"
    # Model the exact published Bun dev pin that conflicts only when Bun is a
    # workspace test root. Its normal dependency must still resolve unchanged.
    published.write_text(manifests["published-bun"].replace(
        '{ path = "../runtime", version = "=1.0.0" }', '"=1.0.0"').replace(
        '{ path = "../optional", version = "=1.0.0", optional = true }',
        '{ version = "=1.0.0", optional = true }').replace(
        '{ path = "../planned", version = "=0.3.20" }', '"=0.3.19"'))
    artifact_manifest = published.read_bytes()
    conflicting = subprocess.run(
        ["cargo", "metadata", "--offline", "--format-version", "1"], cwd=root,
        capture_output=True)
    assert conflicting.returncode != 0, "old exact dev pin unexpectedly accepted as a root"
    staging.stage(root, original, {"published-bun"})
    resolved = metadata()
    after = tomllib.loads((root / "Cargo.lock").read_text())
    staging.validate(before, after, original, {"published-bun"}, [], resolved)
    metadata("--locked")
    assert published.read_bytes() == artifact_manifest, "published manifest was transformed"
    assert {(p["name"], p["version"]) for p in resolved["packages"]} == {
        ("cohort-native", "0.3.20"), ("published-bun", "0.1.17"),
        ("runtime-dep", "1.0.0"), ("optional-dep", "1.0.0")}
    bun = next(p for p in after["package"] if p["name"] == "published-bun")
    assert bun["dependencies"] == ["runtime-dep"], "runtime dependency was lost"

    for field, value in [("version", "0.3.19"),
                         ("source", "git+https://example.invalid/untrusted#deadbeef"),
                         ("checksum", "0" * 64)]:
        malicious = copy.deepcopy(after)
        next(p for p in malicious["package"] if p["name"] == "cohort-native")[field] = value
        try:
            staging.validate(before, malicious, original, {"published-bun"}, [], resolved)
        except ValueError:
            pass
        else:
            raise AssertionError(f"unexpected {field} drift accepted")
    malicious = copy.deepcopy(after)
    next(p for p in malicious["package"] if p["name"] == "published-bun").pop("dependencies")
    try:
        staging.validate(before, malicious, original, {"published-bun"}, [], resolved)
    except ValueError:
        pass
    else:
        raise AssertionError("production dependency removal accepted")

    cohort = [{"package_name": "cohort-native", "version": "0.3.20"}]
    converted = copy.deepcopy(after)
    for package in converted["package"]:
        if package["name"] != "cohort-native":
            package["source"] = "registry+https://github.com/rust-lang/crates.io-index"
            package["checksum"] = "1" * 64
    prefetched = copy.deepcopy(converted)
    staging.validate_registry_conversion(after, converted, prefetched, cohort)
    projected = tomllib.loads(staging.project_clean_room(after, prefetched, cohort))
    staging.validate_registry_conversion(after, projected, prefetched, cohort)
    assert {p["name"] for p in projected["package"]} == {
        "cohort-native", "published-bun", "runtime-dep"}, "unrelated roots were retained"
    assert next(p for p in projected["package"] if p["name"] == "published-bun")["checksum"] == "1" * 64
    optional_before = copy.deepcopy(converted)
    next(p for p in optional_before["package"] if p["name"] == "published-bun")["dependencies"] += ["optional-dep"]
    optional_metadata = {"packages": [], "workspace_members": ["cohort-native"],
                         "resolve": {"nodes": []}}
    for package in projected["package"]:
        record = {**package, "id": package["name"], "dependencies": []}
        for dependency in package.get("dependencies", []):
            record["dependencies"].append({"name": dependency, "kind": None, "optional": False})
        if package["name"] == "published-bun":
            record["dependencies"].append({"name": "optional-dep", "kind": None, "optional": True})
        optional_metadata["packages"].append(record)
        optional_metadata["resolve"]["nodes"].append({"id": package["name"],
            "deps": [{"pkg": d} for d in package.get("dependencies", [])]})
    staging.validate(optional_before, projected, optional_metadata,
                     {"published-bun", "runtime-dep"}, [], optional_metadata, registry_optional=True)
    malicious = copy.deepcopy(projected)
    next(p for p in malicious["package"] if p["name"] == "published-bun").pop("dependencies")
    try:
        staging.validate(optional_before, malicious, optional_metadata,
                         {"published-bun", "runtime-dep"}, [], optional_metadata, registry_optional=True)
    except ValueError:
        pass
    else:
        raise AssertionError("registry optional pruning removed an active runtime dependency")
    for mutation in ("checksum", "version", "edge"):
        malicious = copy.deepcopy(converted)
        package = next(p for p in malicious["package"] if p["name"] == "published-bun")
        if mutation == "edge":
            package["dependencies"] = ["optional-dep"]
        else:
            package[mutation] = "2" * 64 if mutation == "checksum" else "0.1.18"
        try:
            staging.validate_registry_conversion(after, malicious, prefetched, cohort)
        except ValueError:
            pass
        else:
            raise AssertionError(f"clean-room {mutation} drift accepted")

with tempfile.TemporaryDirectory(prefix="lenso-resumed-prefetch-") as directory:
    root = Path(directory)
    registry = "registry+https://github.com/rust-lang/crates.io-index"

    def fetched(name, version, checksum, dependency=None):
        path = root / f"{name}-{version}"
        path.mkdir(exist_ok=True)
        package = {"name": name, "version": version, "source": registry,
                   "id": f"{registry}#{name}@{version}", "manifest_path": f"/registry/{name}-{version}/Cargo.toml"}
        (path / "metadata.json").write_text(json.dumps({"packages": [package]}))
        locked = {"name": name, "version": version, "source": registry, "checksum": checksum}
        if dependency:
            locked["dependencies"] = [dependency]
        lines = ["version = 4", "[[package]]"]
        lines += [f"{key} = {json.dumps(value)}" for key, value in locked.items()]
        (path / "Cargo.lock").write_text("\n".join(lines))
        return path

    # Distinct exact acquisition roots need not be one resolvable application.
    # Keep both compatible old/new codegen identities in the input catalog;
    # only the later validated staging graph selects the approved version.
    fetched("codegen", "0.10.1", "1" * 64, "old-endpoint")
    fetched("codegen", "0.10.2", "2" * 64, "new-endpoint")
    metadata = staging.merge_prefetch(root)
    catalog = tomllib.loads((root / "Cargo.lock").read_text())
    assert {p["version"] for p in metadata["packages"]} == {"0.10.1", "0.10.2"}
    assert all("dependencies" not in p for p in catalog["package"])
    before = {"version": 4, "package": [{"name": "remaining", "version": "0.1.0",
        "dependencies": ["codegen"]}, {"name": "codegen", "version": "0.10.2"}]}
    cohort = [{"package_name": "remaining", "version": "0.1.0"}]
    projected = tomllib.loads(staging.project_clean_room(before, catalog, cohort))
    assert next(p for p in projected["package"] if p["name"] == "codegen")["version"] == "0.10.2"
    for field, value in [("version", "0.10.1"), ("source", "git+https://example.invalid/codegen")]:
        malicious = copy.deepcopy(projected)
        next(p for p in malicious["package"] if p["name"] == "codegen")[field] = value
        try:
            staging.validate_registry_conversion(before, malicious, catalog, cohort)
        except ValueError:
            pass
        else:
            raise AssertionError(f"resumed cohort accepted wrong {field}")
    duplicate = root / "duplicate"
    duplicate.mkdir()
    shutil_source = root / "codegen-0.10.2"
    (duplicate / "metadata.json").write_bytes((shutil_source / "metadata.json").read_bytes())
    (duplicate / "Cargo.lock").write_text((shutil_source / "Cargo.lock").read_text().replace("2" * 64, "3" * 64))
    try:
        staging.merge_prefetch(root)
    except ValueError:
        pass
    else:
        raise AssertionError("inconsistent registry checksum accepted")

print("external dev roots, resumed registry inputs, immutable manifests and strict drift rejection passed")
