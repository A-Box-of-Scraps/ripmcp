import os
import subprocess
import tempfile
import unittest
from contextlib import chdir
from unittest.mock import patch

from scripts.src.validate_ref import main, validate_ref


class RefTests(unittest.TestCase):
    def setUp(self):
        directory = self.enterContext(tempfile.TemporaryDirectory())
        self.enterContext(chdir(directory))
        self.git("init", "-q", "--initial-branch=trunk")
        self.git("config", "user.name", "Release Tests")
        self.git("config", "user.email", "release@example.com")
        self.commit("Initial")
        self.git("update-ref", "refs/remotes/origin/trunk", "HEAD")
        self.environment = {
            "RELEASE_BRANCH": "trunk",
            "GITHUB_EVENT_NAME": "push",
            "GITHUB_REF": "refs/tags/v1.2.3",
            "GITHUB_SHA": self.git("rev-parse", "HEAD"),
        }

    def git(self, *arguments):
        return subprocess.run(
            ["git", *arguments], check=True, capture_output=True, text=True
        ).stdout.strip()

    def commit(self, message):
        self.git(
            "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", message
        )

    def test_tag_at_default_branch_tip(self):
        validate_ref(self.environment)

    def test_tag_on_ancestor(self):
        self.commit("Next")
        self.git("update-ref", "refs/remotes/origin/trunk", "HEAD")
        validate_ref(self.environment)

    def test_annotated_tag(self):
        self.git("-c", "tag.gpgsign=false", "tag", "-a", "v1.2.3", "-m", "Release")
        validate_ref(self.environment | {"GITHUB_SHA": self.git("rev-parse", "v1.2.3")})

    def test_unmerged_commit_is_rejected(self):
        self.git("checkout", "-q", "-b", "feature")
        self.commit("Feature")
        environment = self.environment | {"GITHUB_SHA": self.git("rev-parse", "HEAD")}
        with self.assertRaisesRegex(ValueError, "must belong to the default branch"):
            validate_ref(environment)

    def test_manual_default_branch(self):
        validate_ref(
            self.environment
            | {
                "GITHUB_EVENT_NAME": "workflow_dispatch",
                "GITHUB_REF": "refs/heads/trunk",
            }
        )

    def test_manual_other_refs_are_rejected(self):
        for ref in ["refs/heads/feature", "refs/tags/v1.2.3"]:
            with (
                self.subTest(ref=ref),
                self.assertRaisesRegex(ValueError, "Manual releases"),
            ):
                validate_ref(
                    self.environment
                    | {
                        "GITHUB_EVENT_NAME": "workflow_dispatch",
                        "GITHUB_REF": ref,
                    }
                )

    def test_other_events_and_push_refs_are_rejected(self):
        for changes in [
            {"GITHUB_EVENT_NAME": "pull_request"},
            {"GITHUB_REF": "refs/heads/trunk"},
            {"GITHUB_REF": "refs/tags/nightly"},
        ]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                validate_ref(self.environment | changes)

    def test_missing_remote_branch_fails(self):
        self.git("update-ref", "-d", "refs/remotes/origin/trunk")
        with (
            patch.dict(os.environ, self.environment),
            patch("scripts.src.validate_ref.sys.stderr"),
            self.assertRaises(SystemExit) as result,
        ):
            main()
        self.assertEqual(result.exception.code, 1)


if __name__ == "__main__":
    unittest.main()
