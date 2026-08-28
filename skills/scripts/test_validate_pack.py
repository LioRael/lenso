from __future__ import annotations

import importlib.util
import shutil
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("validate-pack.py")
SPEC = importlib.util.spec_from_file_location("validate_pack", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
VALIDATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VALIDATOR)
CANONICAL_ROOT = SCRIPT.parents[1]


class ValidatePackTests(unittest.TestCase):
    def copy_pack(self) -> tuple[tempfile.TemporaryDirectory[str], Path]:
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name) / "skills"
        shutil.copytree(CANONICAL_ROOT, root)
        return temporary, root

    def test_canonical_pack_passes(self) -> None:
        self.assertEqual(VALIDATOR.validate_pack(CANONICAL_ROOT), [])

    def test_stale_bun_api_fails(self) -> None:
        temporary, root = self.copy_pack()
        self.addCleanup(temporary.cleanup)
        document = root / "lenso-plugin-authoring/references/authoring.md"
        document.write_text(
            document.read_text(encoding="utf-8") + "\nUse defineModule here.\n",
            encoding="utf-8",
        )
        self.assertTrue(
            any("stale `defineModule`" in error for error in VALIDATOR.validate_pack(root))
        )

    def test_router_must_remain_user_invoked(self) -> None:
        temporary, root = self.copy_pack()
        self.addCleanup(temporary.cleanup)
        document = root / "lenso-start/SKILL.md"
        document.write_text(
            document.read_text(encoding="utf-8").replace(
                "disable-model-invocation: true\n", ""
            ),
            encoding="utf-8",
        )
        self.assertIn(
            "lenso-start: router must be user-invoked",
            VALIDATOR.validate_pack(root),
        )

    def test_host_selection_anchor_is_required(self) -> None:
        temporary, root = self.copy_pack()
        self.addCleanup(temporary.cleanup)
        document = root / "lenso-app-configuration/references/resolution.md"
        document.write_text(
            document.read_text(encoding="utf-8").replace(
                "Host policy selects one", "The runtime selects one"
            ),
            encoding="utf-8",
        )
        self.assertTrue(
            any(
                "missing semantic anchor `Host policy selects one`" in error
                for error in VALIDATOR.validate_pack(root)
            )
        )


if __name__ == "__main__":
    unittest.main()
