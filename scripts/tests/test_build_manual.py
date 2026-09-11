import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from scripts.src.build_manual import ROOT, build, generate


class ManualTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def test_committed_manual_is_current(self):
        build(ROOT, check=True)

    def test_generation_uses_manifest_version_without_build_date(self):
        source = self.root / "docs/reference/manual.md"
        source.parent.mkdir(parents=True)
        source.write_text("# NAME\n\nripmcp - test manual\n")
        (self.root / "Cargo.toml").write_text('[package]\nversion = "9.8.7"\n')
        first = generate(self.root)
        self.assertIn(b'.TH "RIPMCP" "1" "" "ripmcp 9.8.7"', first)
        with patch.dict(os.environ, {"SOURCE_DATE_EPOCH": "1"}):
            self.assertEqual(generate(self.root), first)

    @patch("scripts.src.build_manual.generate", return_value=b"generated\n")
    def test_write_and_check_missing_stale_and_current_output(self, generate_page):
        destination = self.root / "man/ripmcp.1"
        with self.assertRaisesRegex(ValueError, "stale"):
            build(self.root, check=True)
        self.assertFalse(destination.exists())
        build(self.root, check=False)
        self.assertEqual(destination.read_bytes(), generate_page.return_value)
        destination.write_bytes(b"stale\n")
        with self.assertRaisesRegex(ValueError, "stale"):
            build(self.root, check=True)
        self.assertEqual(destination.read_bytes(), b"stale\n")
        build(self.root, check=False)
        modified = destination.stat().st_mtime_ns
        build(self.root, check=True)
        self.assertEqual(destination.stat().st_mtime_ns, modified)

    @patch("scripts.src.build_manual.subprocess.run")
    def test_converter_version_is_pinned(self, run):
        for version in ["", "pandoc 2.0\n", "pandoc 3.11.1\n"]:
            with self.subTest(version=version):
                run.reset_mock()
                run.return_value.stdout = version
                with self.assertRaisesRegex(ValueError, "requires Pandoc 3.11"):
                    generate(self.root)
                run.assert_called_once()

    def test_rendering_has_no_warnings_or_overlong_lines(self):
        rendered = subprocess.run(
            [
                "groff",
                "-Tascii",
                "-man",
                "-ww",
                "-P-c",
                "-P-b",
                "-P-u",
                "-rLL=78n",
                "-rLT=78n",
                str(ROOT / "man/ripmcp.1"),
            ],
            check=True,
            capture_output=True,
            text=True,
            env=os.environ | {"LC_ALL": "C", "GROFF_NO_SGR": "1"},
        )
        self.assertEqual(rendered.stderr, "")
        self.assertNotIn("\x1b", rendered.stdout)
        for line in rendered.stdout.splitlines():
            self.assertLessEqual(len(line), 80, line)
        for heading in ["QUICK START", "AUTHENTICATION", "EXIT STATUS", "FILES"]:
            self.assertIn(heading, rendered.stdout)
        self.assertIn("ripmcp call SERVER TOOL --input arguments.json", rendered.stdout)

    def test_user_local_installation_and_noninteractive_lookup(self):
        manual_home = self.root / ".local/share/man"
        installed = manual_home / "man1/ripmcp.1"
        subprocess.run(
            ["install", "-Dm644", str(ROOT / "man/ripmcp.1"), str(installed)],
            check=True,
        )
        environment = {
            "PATH": os.environ["PATH"],
            "HOME": str(self.root),
            "MANPATH": f"{manual_home}:",
            "MANPAGER": "cat",
            "MANWIDTH": "80",
            "LC_ALL": "C",
        }
        location = subprocess.run(
            ["man", "-w", "1", "ripmcp"],
            check=True,
            capture_output=True,
            text=True,
            env=environment,
        )
        self.assertEqual(Path(location.stdout.strip()), installed)
        rendered = subprocess.run(
            ["man", "1", "ripmcp"],
            check=True,
            capture_output=True,
            text=True,
            env=environment,
        )
        self.assertEqual(rendered.stderr, "")
        self.assertIn("QUICK START", rendered.stdout)
        self.assertEqual(installed.stat().st_mode & 0o777, 0o644)

    def test_manual_has_no_archive_or_relative_markdown_links(self):
        source = (ROOT / "docs/reference/manual.md").read_text()
        self.assertNotIn("old/", source)
        self.assertNotRegex(source, r"\]\((?!https?://)")
        self.assertTrue(source.isascii())


if __name__ == "__main__":
    unittest.main()
