"""Unit tests for src/internal/skill_new.py.

Run from the repo root with:

    uv run --project src/internal python -m unittest discover -s src/internal -p "test_*.py"
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import lib
import skill_new


def _seed_repo(root: Path) -> None:
    """Create a minimal registry layout inside ``root``: empty mapping.yml + dirs."""
    (root / "mapping.yml").write_text(
        "installed_skills: []\ninstalled_agents_md: []\n",
        encoding="utf-8",
    )
    (root / "assets" / "skills").mkdir(parents=True, exist_ok=True)
    (root / "assets" / "agents.md").mkdir(parents=True, exist_ok=True)


class SkillNewTests(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        _seed_repo(self.root)
        self._patches = [
            patch.object(lib, "repo_root", return_value=self.root),
            patch.object(skill_new, "repo_root", return_value=self.root),
        ]
        for p in self._patches:
            p.start()

    def tearDown(self):
        for p in self._patches:
            p.stop()
        self._tmp.cleanup()

    def _run(
        self, argv: list[str], inputs: list[str], *, tty: bool = True
    ) -> int:
        iterator = iter(inputs)
        with patch.object(
            sys.stdin, "isatty", return_value=tty
        ), patch(
            "builtins.input",
            side_effect=lambda *_: next(iterator),
        ):
            return skill_new.main(argv)

    def test_creates_skill_with_all_fields(self):
        rc = self._run(
            ["my-skill"],
            inputs=["a fine skill", "0.2.1", "1,4"],
        )
        self.assertEqual(rc, 0)
        skill_dir = self.root / "assets" / "skills" / "my-skill"
        self.assertTrue((skill_dir / "SKILL.md").is_file())
        self.assertIn(
            "TODO: describe the skill",
            (skill_dir / "SKILL.md").read_text(encoding="utf-8"),
        )
        manifest_text = (skill_dir / "manifest.yml").read_text(encoding="utf-8")
        self.assertIn("name: my-skill", manifest_text)
        self.assertIn("description: a fine skill", manifest_text)
        self.assertIn("version: 0.2.1", manifest_text)
        self.assertIn("claude-code", manifest_text)
        self.assertIn("codebuff", manifest_text)
        self.assertIn("entrypoint: SKILL.md", manifest_text)

        mapping_text = (self.root / "mapping.yml").read_text(encoding="utf-8")
        self.assertIn("name: my-skill", mapping_text)
        self.assertIn("source_url: local", mapping_text)
        self.assertIn("install_path: assets/skills/my-skill/", mapping_text)

    def test_default_version_used_on_empty_input(self):
        rc = self._run(["alpha"], inputs=["desc here", "", ""])
        self.assertEqual(rc, 0)
        manifest_text = (
            self.root / "assets" / "skills" / "alpha" / "manifest.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("version: 0.1.0", manifest_text)
        self.assertIn("harness_compatibility: []", manifest_text)

    def test_invalid_semver_reprompts(self):
        rc = self._run(
            ["beta"],
            inputs=["d", "not-a-version", "1.0.0", ""],
        )
        self.assertEqual(rc, 0)
        manifest_text = (
            self.root / "assets" / "skills" / "beta" / "manifest.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("version: 1.0.0", manifest_text)

    def test_empty_description_reprompts(self):
        rc = self._run(
            ["gamma"],
            inputs=["", "   ", "real description", "", ""],
        )
        self.assertEqual(rc, 0)
        manifest_text = (
            self.root / "assets" / "skills" / "gamma" / "manifest.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("description: real description", manifest_text)

    def test_name_sanitized(self):
        rc = self._run(["My Cool Skill!"], inputs=["d", "", ""])
        self.assertEqual(rc, 0)
        self.assertTrue(
            (self.root / "assets" / "skills" / "my-cool-skill").is_dir()
        )

    def test_collision_without_force_raises(self):
        (self.root / "assets" / "skills" / "dup").mkdir()
        with patch.object(
            sys.stdin, "isatty", return_value=True
        ), self.assertRaises(SystemExit) as ctx:
            skill_new.main(["dup"])
        self.assertNotEqual(ctx.exception.code, 0)

    def test_force_overwrites_existing(self):
        existing = self.root / "assets" / "skills" / "dup"
        existing.mkdir()
        (existing / "old.txt").write_text("old", encoding="utf-8")
        rc = self._run(["dup", "--force"], inputs=["fresh description", "", ""])
        self.assertEqual(rc, 0)
        self.assertFalse((self.root / "assets" / "skills" / "dup" / "old.txt").exists())
        self.assertTrue((self.root / "assets" / "skills" / "dup" / "SKILL.md").is_file())

    def test_non_tty_fails_fast(self):
        with patch.object(
            sys.stdin, "isatty", return_value=False
        ), self.assertRaises(SystemExit) as ctx:
            skill_new.main(["whatever"])
        self.assertNotEqual(ctx.exception.code, 0)


if __name__ == "__main__":
    unittest.main()
