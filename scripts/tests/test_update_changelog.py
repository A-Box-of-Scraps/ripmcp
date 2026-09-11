import os
import subprocess
import tempfile
import unittest
from contextlib import chdir
from pathlib import Path
from unittest.mock import patch

from scripts.src.update_changelog import main, next_cycle


class ChangelogTests(unittest.TestCase):
    def setUp(self):
        self.changelog = "# Changelog\n\n## [Unreleased]\n\n### Added\n\n- CLI\n"

    def test_rollover_preserves_entries(self):
        updated = next_cycle(self.changelog, "v1.2.3", "2026-09-11")
        self.assertEqual(
            updated,
            self.changelog.replace(
                "## [Unreleased]",
                "## [Unreleased]\n\n## [1.2.3] - 2026-09-11",
            ),
        )
        self.assertEqual(next_cycle(updated, "v1.2.3", "2026-09-12"), updated)

    def test_already_versioned_changelog(self):
        released = self.changelog.replace("[Unreleased]", "[1.2.3] - 2026-09-11")
        self.assertEqual(
            next_cycle(released, "v1.2.3", "2026-09-11"),
            released.replace("## [1.2.3]", "## [Unreleased]\n\n## [1.2.3]"),
        )

    def test_invalid_headings(self):
        for changelog in ["# Changelog\n", self.changelog * 2]:
            with self.subTest(changelog=changelog), self.assertRaises(ValueError):
                next_cycle(changelog, "v1.2.3", "2026-09-11")

    def test_invalid_versions(self):
        for tag in ["v", "vbad\ntag", "v[bad]"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                next_cycle(self.changelog, tag, "2026-09-11")


class ChangelogCommandTests(unittest.TestCase):
    def setUp(self):
        directory = self.enterContext(tempfile.TemporaryDirectory())
        self.enterContext(chdir(directory))
        self.enterContext(
            patch.dict(os.environ, RELEASE_TAG="v1.2.3", RELEASE_BRANCH="main")
        )
        self.git_run = self.enterContext(
            patch("scripts.src.update_changelog.subprocess.run")
        )
        Path("next-cycle").mkdir()
        self.changelog = "# Changelog\n\n## [Unreleased]\n\n- CLI\n"
        Path("CHANGELOG.md").write_text(self.changelog)
        self.target = Path("next-cycle/CHANGELOG.md")
        self.target.write_text(self.changelog)

    def test_commit_and_push(self):
        main()
        self.assertIn("## [1.2.3] - ", self.target.read_text())
        self.git_run.assert_any_call(
            ["git", "commit", "-m", "Add [Unreleased] section for next cycle"],
            cwd=Path("next-cycle"),
            check=True,
        )
        self.assertEqual(
            self.git_run.call_args.args[0],
            ["git", "push", "origin", "HEAD:refs/heads/main"],
        )

    def test_existing_cycle_is_unchanged(self):
        updated = next_cycle(self.changelog, "v1.2.3", "2026-09-11")
        self.target.write_text(updated)
        main()
        self.assertEqual(self.target.read_text(), updated)
        self.git_run.assert_not_called()

    def test_concurrent_change_is_preserved(self):
        changed = self.changelog + "\n- Concurrent change\n"
        self.target.write_text(changed)
        with (
            patch("scripts.src.update_changelog.sys.stderr"),
            self.assertRaises(SystemExit),
        ):
            main()
        self.assertEqual(self.target.read_text(), changed)
        self.git_run.assert_not_called()

    def test_git_failure_stops_update(self):
        self.git_run.side_effect = subprocess.CalledProcessError(1, "git")
        with (
            patch("scripts.src.update_changelog.sys.stderr"),
            self.assertRaises(SystemExit),
        ):
            main()
        self.assertEqual(self.git_run.call_count, 1)


if __name__ == "__main__":
    unittest.main()
