import subprocess
import tempfile
import unittest
from contextlib import chdir
from pathlib import Path
from unittest.mock import patch

from scripts.src.create_release import SEMVER, main, release_command, release_notes


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.enterContext(chdir(self.temporary.name))
        self.manifest = Path(self.temporary.name) / "Cargo.toml"
        self.manifest.write_text('[package]\nversion = "1.2.3"\n')
        self.notes = "### Added\n\n- New command\n\n### Fixed\n\n- Bug fix\n"
        Path("CHANGELOG.md").write_text(
            "# Changelog\n\n## [Unreleased]\n\n" + self.notes
        )
        self.environment = {
            "GITHUB_EVENT_NAME": "push",
            "GITHUB_REF_TYPE": "tag",
            "GITHUB_REF_NAME": "v1.2.3",
            "GITHUB_REPOSITORY": "owner/repo",
            "GITHUB_SHA": "a" * 40,
        }

    def command(self, **changes):
        return release_command(self.environment | changes, self.manifest)

    def test_semver(self):
        cases = {
            True: [
                "0.0.0",
                "1.2.3",
                "10.20.30",
                "1.2.3-alpha.1",
                "1.2.3-0",
                "1.2.3-01a",
                "1.2.3--",
                "1.2.3+001",
                "1.2.3-rc.1+build.01",
            ],
            False: [
                "",
                "1",
                "1.2",
                "v1.2.3",
                "01.2.3",
                "1.02.3",
                "1.2.03",
                "1.2.3-01",
                "1.2.3-alpha.01",
                "1.2.3-",
                "1.2.3+",
                "1.2.3-a..b",
                "1.2.3+a..b",
                "1.2.3_alpha",
                "1.2.3\n",
                "1.2.3-\u00e9",
                "\u0661.2.3",
                " 1.2.3",
            ],
        }
        for valid, versions in cases.items():
            for version in versions:
                with self.subTest(version=version):
                    self.assertEqual(SEMVER.fullmatch(version) is not None, valid)

    def test_push(self):
        command = self.command(RELEASE_VERSION="nightly", RELEASE_DRAFT="true")
        self.assertEqual(command[:4], ["gh", "release", "create", "v1.2.3"])
        self.assertIn("--verify-tag", command)
        self.assertNotIn("--generate-notes", command)
        self.assertEqual(command[command.index("--notes-file") + 1], "-")
        self.assertNotIn("--draft", command)
        self.assertNotIn("--prerelease", command)
        self.assertEqual(command[command.index("--target") + 1], "a" * 40)
        self.assertEqual(command[command.index("--repo") + 1], "owner/repo")

    def test_invalid_automatic_releases(self):
        for tag in ["v1.2", "v01.2.3", "v1.2.3-01", "nightly", "1.2.3"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                self.command(GITHUB_REF_NAME=tag)
        with self.assertRaises(ValueError):
            self.command(GITHUB_REF_TYPE="branch")
        with self.assertRaises(ValueError):
            self.command(GITHUB_EVENT_NAME="pull_request")

    def test_manual_branch_defaults_to_manifest(self):
        command = self.command(
            GITHUB_EVENT_NAME="workflow_dispatch",
            GITHUB_REF_TYPE="branch",
            GITHUB_REF_NAME="main",
            RELEASE_DRAFT="true",
        )
        self.assertEqual(command[3], "v1.2.3")
        self.assertIn("--draft", command)
        self.assertNotIn("--verify-tag", command)

    def test_manual_tag_defaults_to_selected_tag(self):
        self.manifest.write_text('[package]\nversion = "2.0.0"\n')
        command = self.command(
            GITHUB_EVENT_NAME="workflow_dispatch",
            GITHUB_REF_NAME="v2.0.0",
        )
        self.assertEqual(command[3], "v2.0.0")
        self.assertIn("--verify-tag", command)

    @patch("scripts.src.create_release.subprocess.run")
    def test_tag_must_match_manifest(self, run):
        for event in ["push", "workflow_dispatch"]:
            for tag in ["v2.0.0", "v1.2.3-rc.1", "v1.2.3+build.1"]:
                with self.subTest(event=event, tag=tag):
                    with self.assertRaisesRegex(
                        ValueError, "does not match Cargo.toml"
                    ):
                        self.command(GITHUB_EVENT_NAME=event, GITHUB_REF_NAME=tag)
        run.assert_not_called()

    def test_manual_override_bypasses_manifest_match(self):
        command = self.command(
            GITHUB_EVENT_NAME="workflow_dispatch",
            RELEASE_VERSION="v2.0.0",
        )
        self.assertEqual(command[3], "v2.0.0")

    def test_manual_default_still_requires_semver(self):
        with self.assertRaises(ValueError):
            self.command(GITHUB_EVENT_NAME="workflow_dispatch", GITHUB_REF_NAME="vbad")
        self.manifest.write_text('[package]\nversion = "invalid"\n')
        with self.assertRaises(ValueError):
            self.command(
                GITHUB_EVENT_NAME="workflow_dispatch", GITHUB_REF_TYPE="branch"
            )

    def test_manual_override_bypasses_semver(self):
        for override in ["nightly", "1.2", "01.2.3", "v2.0.0"]:
            with self.subTest(override=override):
                command = self.command(
                    GITHUB_EVENT_NAME="workflow_dispatch",
                    RELEASE_VERSION=override,
                )
                expected = override if override.startswith("v") else f"v{override}"
                self.assertEqual(command[3], expected)
                self.assertNotIn("--verify-tag", command)

    def test_override_must_be_valid_git_tag(self):
        for override in ["bad tag", "bad\ntag", "bad..tag", "bad~tag"]:
            with self.subTest(override=override):
                with self.assertRaises(subprocess.CalledProcessError):
                    self.command(
                        GITHUB_EVENT_NAME="workflow_dispatch",
                        RELEASE_VERSION=override,
                    )

    def test_prerelease_detection(self):
        for tag, prerelease in [("v1.2.3-rc.1", True), ("v1.2.3+build-a", False)]:
            with self.subTest(tag=tag):
                self.manifest.write_text(f'[package]\nversion = "{tag[1:]}"\n')
                self.assertEqual(
                    "--prerelease" in self.command(GITHUB_REF_NAME=tag), prerelease
                )

    @patch("scripts.src.create_release.subprocess.run")
    @patch(
        "scripts.src.create_release.release_command",
        return_value=["gh", "release", "create", "v1.2.3"],
    )
    def test_main_creates_release(self, prepare, run):
        with patch.dict("os.environ", self.environment):
            main()
        run.assert_called_once_with(
            prepare.return_value,
            input=self.notes
            + "\n**Full Changelog**: https://github.com/owner/repo/commits/v1.2.3\n",
            text=True,
            check=True,
        )

    @patch("scripts.src.create_release.subprocess.run")
    def test_full_changelog_uses_selected_release_tag(self, run):
        for override in ["2.0.0", "v2.0.0-rc.1", "nightly"]:
            with (
                self.subTest(override=override),
                patch.dict(
                    "os.environ",
                    self.environment
                    | {
                        "GITHUB_EVENT_NAME": "workflow_dispatch",
                        "GITHUB_REPOSITORY": "A-Box-of-Scraps/ripmcp",
                        "RELEASE_VERSION": override,
                    },
                    clear=True,
                ),
            ):
                main()
                tag = override if override.startswith("v") else f"v{override}"
                self.assertEqual(
                    run.call_args.kwargs["input"],
                    self.notes
                    + "\n**Full Changelog**: "
                    + f"https://github.com/A-Box-of-Scraps/ripmcp/commits/{tag}\n",
                )

    @patch("scripts.src.create_release.subprocess.run")
    @patch(
        "scripts.src.create_release.release_command",
        return_value=["gh", "release", "create", "v1.2.3"],
    )
    def test_main_attaches_assets_and_exports_tag(self, prepare, run):
        output = Path(self.temporary.name) / "output"
        with patch.dict(
            "os.environ", self.environment | {"GITHUB_OUTPUT": str(output)}
        ):
            main([str(self.manifest)])
        self.assertEqual(run.call_args.args[0][-1], str(self.manifest))
        self.assertEqual(
            run.call_args.kwargs["input"],
            self.notes
            + "\n**Full Changelog**: https://github.com/owner/repo/commits/v1.2.3\n",
        )
        self.assertEqual(output.read_text(), "tag=v1.2.3\n")

    @patch("scripts.src.create_release.subprocess.run")
    @patch(
        "scripts.src.create_release.release_command",
        return_value=["gh", "release", "create", "v1.2.3"],
    )
    def test_missing_asset_does_not_publish(self, prepare, run):
        with (
            patch("scripts.src.create_release.sys.stderr"),
            self.assertRaises(SystemExit),
        ):
            main([str(self.manifest.parent / "missing.tar.gz")])
        run.assert_not_called()

    @patch("scripts.src.create_release.subprocess.run")
    @patch(
        "scripts.src.create_release.release_command",
        side_effect=ValueError("invalid version"),
    )
    @patch("scripts.src.create_release.sys.stderr")
    def test_validation_failure_does_not_create_release(self, stderr, prepare, run):
        with self.assertRaises(SystemExit) as result:
            main()
        self.assertEqual(result.exception.code, 1)
        run.assert_not_called()

    def test_notes_preserve_categories_and_exclude_previous_releases(self):
        changelog = (
            "# Changelog\n\nIntro\n\n## [Unreleased]\n\n"
            + self.notes
            + "\n## [1.0.0] - 2026-09-01\n\n### Added\n\n- Old command\n"
        )
        self.assertEqual(release_notes(changelog), self.notes)

    def test_notes_without_previous_releases(self):
        self.assertEqual(release_notes(Path("CHANGELOG.md").read_text()), self.notes)

    def test_invalid_notes(self):
        for changelog in [
            "# Changelog\n",
            "## [Unreleased]\n\n## [Unreleased]\n\n- Duplicate\n",
            "## [Unreleased]\n\n",
            "## [Unreleased]\n\n## [1.0.0]\n\n- Old command\n",
        ]:
            with self.subTest(changelog=changelog), self.assertRaises(ValueError):
                release_notes(changelog)

    @patch("scripts.src.create_release.subprocess.run")
    @patch(
        "scripts.src.create_release.release_command",
        return_value=["gh", "release", "create", "v1.2.3"],
    )
    def test_invalid_changelog_does_not_publish(self, prepare, run):
        Path("CHANGELOG.md").write_text("## [Unreleased]\n")
        with (
            patch("scripts.src.create_release.sys.stderr"),
            self.assertRaises(SystemExit),
        ):
            main()
        run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
