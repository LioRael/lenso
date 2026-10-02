#!/usr/bin/env python3
"""Prove real Bun command resolution and fail-fast behavior without Rust builds."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("check.sh").resolve()


class BunCommandTests(unittest.TestCase):
    def environment(self, fixture):
        home = fixture / "home"
        home.mkdir(exist_ok=True)
        return dict(os.environ, HOME=str(home), XDG_CACHE_HOME=str(fixture / "cache"),
                    BUN_INSTALL_CACHE_DIR=str(fixture / "bun-cache"))

    def shell(self, source, fixture):
        return subprocess.run(
            ["bash", "-c", source, "bun-command-proof", str(SCRIPT), str(fixture)],
            env=self.environment(fixture), text=True, capture_output=True, timeout=5,
        )

    def test_real_executable_resolution_install_and_failed_build(self):
        # No executable substitutes: use the installed Bun to install a real
        # zero-dependency project and execute its deliberately failing build.
        executable = shutil.which("bun")
        self.assertIsNotNone(executable, "Install Bun before running this regression")
        version = subprocess.check_output([executable, "--version"], text=True).strip()
        with tempfile.TemporaryDirectory(prefix="lenso-bun-command-") as temporary:
            fixture = Path(temporary)
            (fixture / "package.json").write_text(json.dumps({
                "name": "bun-command-negative", "version": "0.0.0", "private": True,
                "scripts": {"build": "bun -e 'console.error(\"real Bun build rejected\"); process.exit(37)'"},
            }))
            subprocess.run([executable, "install"], cwd=fixture, env=self.environment(fixture),
                           check=True, capture_output=True)
            result = self.shell('''
source "$1"
test "$(type -t bun)" = file
printf 'resolved Bun: %s; version: %s\n' "$(command -v bun)" "$(bun --version)"
LENSO_JS_ROOT="$2"
# This tests the actual command body, independently of the checkout/version
# admission policy. The full entrypoint still checks that policy first.
bun_commands
''', fixture)
            self.assertEqual(result.returncode, 37, result.stdout + result.stderr)
            self.assertIn(executable, result.stdout)
            self.assertIn(version, result.stdout)
            self.assertIn("+ bun install --frozen-lockfile", result.stdout)
            self.assertIn("+ bun run build", result.stdout)
            self.assertIn("real Bun build rejected", result.stderr)
            self.assertNotIn("+ cargo test", result.stdout)
            print(result.stdout + result.stderr, end="")

    def test_function_collision_reproduces_bounded_recursion(self):
        with tempfile.TemporaryDirectory(prefix="lenso-bun-recursion-") as temporary:
            result = self.shell('''
source "$1"
LENSO_JS_ROOT="$2"
# Reintroduce the exact command-resolution cycle from the reviewed defect.
# A depth guard bounds the negative fixture before any build can run.
bun() { test "$(bun --version)" = 1.4.2; }
set -T
trap 'if (( BASH_SUBSHELL > 2 )); then echo "rejected recursive Bun command resolution" >&2; exit 91; fi' DEBUG
bun_inputs
''', Path(temporary))
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("rejected recursive Bun command resolution", result.stderr)
            print(result.stderr, end="")


if __name__ == "__main__":
    unittest.main()
