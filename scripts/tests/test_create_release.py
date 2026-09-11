import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from scripts.src.create_release import SEMVER, main, release_command


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.manifest = Path(self.temporary.name) / "Cargo.toml"
        self.manifest.write_text('[package]\nversion = "1.2.3"\n')
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
        self.assertIn("--generate-notes", command)
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
        main()
        run.assert_called_once_with(prepare.return_value, check=True)

    @patch("scripts.src.create_release.subprocess.run")
    @patch(
        "scripts.src.create_release.release_command",
        return_value=["gh", "release", "create", "v1.2.3"],
    )
    def test_main_attaches_assets_and_exports_tag(self, prepare, run):
        output = Path(self.temporary.name) / "output"
        with patch.dict("os.environ", GITHUB_OUTPUT=str(output)):
            main([str(self.manifest)])
        self.assertEqual(run.call_args.args[0][-1], str(self.manifest))
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


if __name__ == "__main__":
    unittest.main()
