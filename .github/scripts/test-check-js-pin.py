#!/usr/bin/env python3
"""Check the exact cross-repository pin using real local Git checkouts."""
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / ".github/scripts/check.sh"


class JavaScriptPinTests(unittest.TestCase):
    def verify(self, checkout):
        return subprocess.run(
            ["bash", "-c", 'source "$1"; verify_js_revision', "js-pin-proof", str(SCRIPT)],
            env=dict(os.environ, LENSO_JS_ROOT=str(checkout)),
            text=True, capture_output=True, timeout=5,
        )

    def test_workflow_and_script_select_the_same_full_commit(self):
        pin = re.search(r"expected_js_revision=([0-9a-f]{40})\b", SCRIPT.read_text())
        self.assertIsNotNone(pin)
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        checkout = re.search(r"repository: LioRael/lenso-js\s+ref: ([0-9a-f]{40})\b", workflow)
        self.assertIsNotNone(checkout)
        self.assertEqual(checkout.group(1), pin.group(1))
        self.assertIn(f'$(git -C lenso-js rev-parse HEAD)" = {pin.group(1)}', workflow)

    def test_selected_checkout_is_accepted(self):
        result = self.verify(os.environ["LENSO_JS_ROOT"])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_other_real_commit_is_rejected(self):
        with tempfile.TemporaryDirectory(prefix="lenso-js-pin-") as directory:
            subprocess.run(["git", "init", "--quiet", directory], check=True)
            subprocess.run([
                "git", "-C", directory, "-c", "user.name=Pin fixture",
                "-c", "user.email=fixture@example.invalid", "commit", "--quiet",
                "--allow-empty", "-m", "test: unrelated checkout",
            ], check=True)
            result = self.verify(directory)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Expected lenso-js", result.stderr)


if __name__ == "__main__":
    unittest.main()
