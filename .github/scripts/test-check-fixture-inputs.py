#!/usr/bin/env python3
"""Negative fixtures for missing/stale locks and Web version assertion drift."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("inputs", Path(__file__).with_name("check-fixture-inputs.py"))
inputs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(inputs)


class FixtureInputsTests(unittest.TestCase):
    def test_missing_and_stale_lock_fail_without_repair(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "src").mkdir()
            (root / "src/lib.rs").write_text("")
            manifest = root / "Cargo.toml"
            manifest.write_text('[package]\nname="lock-negative"\nversion="0.1.0"\nedition="2024"\n[workspace]\n')
            with self.assertRaisesRegex(RuntimeError, "missing prepared fixture lock"):
                inputs.check_locks(root, [])
            subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=root, check=True)
            inputs.check_locks(root, [])
            lock = (root / "Cargo.lock").read_bytes()
            manifest.write_text(manifest.read_text().replace('version="0.1.0"', 'version="0.2.0"'))
            with self.assertRaises(subprocess.CalledProcessError):
                inputs.check_locks(root, [])
            self.assertEqual((root / "Cargo.lock").read_bytes(), lock)

    def test_missing_standalone_guest_lock_is_not_skipped(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text("[workspace]\nmembers=[]\n")
            (root / "Cargo.lock").write_text("version=4\n")
            guest = root / "guest/Cargo.toml"
            guest.parent.mkdir()
            guest.write_text('[workspace]\n[lib]\ncrate-type=["cdylib"]\n')
            with self.assertRaisesRegex(RuntimeError, "guest/Cargo.lock"):
                inputs.check_locks(root, [guest])

    def test_scaffold_and_independent_expectation_drift_fail(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            directory = root / "crates/lenso-engine-app/src/plugin"
            directory.mkdir(parents=True)
            for name in ["scaffold.rs", "tests.rs", "web_dev.rs"]:
                (directory / name).write_bytes((inputs.ROOT / "crates/lenso-engine-app/src/plugin" / name).read_bytes())
            import re
            source = (directory / "scaffold.rs").read_text()
            dev_source = (directory / "web_dev.rs").read_text()
            names = set(re.findall(r'^(lenso[\w-]*) = "=', source, re.M))
            names.update(re.findall(r'"(lenso[\w-]*)" => "[^"]+"', dev_source))
            names.add("lenso-capability-http-endpoint")
            for name in names:
                path = root / "crates" / name / "Cargo.toml"
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes((inputs.ROOT / "crates" / name / "Cargo.toml").read_bytes())
            inputs.check_web_cohort(root)
            endpoint = directory / "scaffold.rs"
            endpoint.write_text(source.replace('VERSION: &str = "0.3.8"', 'VERSION: &str = "999.0.0"'))
            with self.assertRaisesRegex(RuntimeError, "source is"):
                inputs.check_web_cohort(root)
            endpoint.write_text(source)
            tests = directory / "tests.rs"
            tests.write_text(tests.read_text().replace('("lenso-web-host", "=0.2.6")', '("lenso-web-host", "=999.0.0")'))
            with self.assertRaisesRegex(RuntimeError, "expectation drift"):
                inputs.check_web_cohort(root)


if __name__ == "__main__":
    unittest.main()
