#!/usr/bin/env python3
"""Reject stale prepared fixture inputs before compiling the workspace."""
from pathlib import Path
import re
import subprocess
import tomllib


ROOT = Path(__file__).resolve().parents[2]


def tracked_manifests(root):
    names = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", "**/Cargo.toml"], cwd=root
    ).decode().split("\0")
    return [root / name for name in names if name]


def check_locks(root, manifests):
    # Standalone executable Wasm fixtures require prepared locks just like the
    # workspace. Source-only snippets copied into a generated project do not.
    selected = [root / "Cargo.toml"]
    for path in manifests:
        manifest = tomllib.loads(path.read_text())
        if "workspace" in manifest and (
            "cdylib" in manifest.get("lib", {}).get("crate-type", [])
            or path.with_name("Cargo.lock").is_file()
        ):
            selected.append(path)
    for path in selected:
        if not path.with_name("Cargo.lock").is_file():
            raise RuntimeError(f"missing prepared fixture lock: {path.with_name('Cargo.lock').relative_to(root)}")
    for path in sorted(set(selected)):
        lock = path.with_name("Cargo.lock")
        if not lock.is_file():
            raise RuntimeError(f"missing prepared fixture lock: {lock.relative_to(root)}")
        original = lock.read_bytes()
        print(f"Locked metadata: {path.relative_to(root)}", flush=True)
        subprocess.run(
            ["cargo", "metadata", "--locked", "--format-version", "1",
             "--manifest-path", str(path)], cwd=root, stdout=subprocess.DEVNULL, check=True
        )
        if lock.read_bytes() != original:
            raise RuntimeError(f"locked metadata mutated {lock}")


def check_web_cohort(root):
    directory = root / "crates/lenso-engine-app/src/plugin"
    source = (directory / "scaffold.rs").read_text()
    endpoint = re.search(r'WEB_HTTP_ENDPOINT_VERSION: &str = "([^"]+)"', source)
    if endpoint is None:
        raise RuntimeError("missing Web Endpoint cohort declaration")
    manifest_source = source.split("fn web_plugin_scaffold(", 1)[1].split('"#', 1)[0]
    pins = dict(re.findall(r'^(lenso[\w-]*) = "=([^"{}]+)"$', manifest_source, re.M))
    pins["lenso-capability-http-endpoint"] = endpoint[1]
    if len(pins) != 7:
        raise RuntimeError("Web scaffold cohort shape changed; review its validation")
    assertions = (directory / "tests.rs").read_text().split(
        "fn web_plugin_scaffold_uses_canonical_endpoint_authoring()", 1
    )[1].split("#[test]", 1)[0]
    expectations = dict(re.findall(r'\("(lenso[\w-]*)", "=([^"]+)"\)', assertions))
    for name, version in pins.items():
        actual = tomllib.loads((root / "crates" / name / "Cargo.toml").read_text())["package"]["version"]
        if version != actual:
            raise RuntimeError(f"Web scaffold {name} pins {version}; source is {actual}")
        # Keep the independent handwritten assertion; do not generate expected
        # values from the scaffold and silently accept a contract change.
        if name != "lenso-engine-web" and expectations.get(name) != version:
            raise RuntimeError(f"Web scaffold test expectation drift: {name}@{version}")
    dev_source = (directory / "web_dev.rs").read_text()
    dev_pins = dict(re.findall(r'"(lenso[\w-]*)" => "([^"]+)"', dev_source))
    for name, version in dev_pins.items():
        actual = tomllib.loads((root / "crates" / name / "Cargo.toml").read_text())["package"]["version"]
        if version != actual:
            raise RuntimeError(f"Web development Host {name} pins {version}; source is {actual}")
    print("Web scaffold, Endpoint requirement and independent cohort assertions agree")


if __name__ == "__main__":
    check_web_cohort(ROOT)
    check_locks(ROOT, tracked_manifests(ROOT))
