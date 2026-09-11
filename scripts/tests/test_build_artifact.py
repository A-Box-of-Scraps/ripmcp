import os
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class ArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for name in ["scripts/src", "man", "bin"]:
            (self.root / name).mkdir(parents=True)
        self.script = self.root / "scripts/src/build_artifact.sh"
        shutil.copyfile(ROOT / "scripts/src/build_artifact.sh", self.script)
        shutil.copyfile(ROOT / "man/ripmcp.1", self.root / "man/ripmcp.1")
        cargo = self.root / "bin/cargo"
        cargo.write_text(
            "#!/bin/sh\nset -eu\n"
            'test "$*" = "build --locked --release --bin ripmcp"\n'
            "mkdir -p target/release\n"
            'printf "test binary\\n" > target/release/ripmcp\n'
            "chmod 755 target/release/ripmcp\n"
        )
        cargo.chmod(0o755)
        self.environment = os.environ | {
            "PATH": f"{self.root / 'bin'}:{os.environ['PATH']}"
        }

    def package(self):
        return subprocess.run(
            ["bash", str(self.script)],
            cwd=self.root / "bin",
            env=self.environment,
            capture_output=True,
            text=True,
        )

    def test_release_contains_binary_and_matching_manual_with_checksum(self):
        result = self.package()
        self.assertEqual(result.returncode, 0, result.stderr)
        dist = self.root / "dist"
        archive = dist / "ripmcp-x86_64-unknown-linux-gnu.tar.gz"
        with tarfile.open(archive) as package:
            self.assertEqual(set(package.getnames()), {"ripmcp", "man/ripmcp.1"})
            self.assertEqual(package.extractfile("ripmcp").read(), b"test binary\n")
            self.assertEqual(package.getmember("ripmcp").mode, 0o755)
            self.assertEqual(
                package.extractfile("man/ripmcp.1").read(),
                (ROOT / "man/ripmcp.1").read_bytes(),
            )
        subprocess.run(
            ["sha256sum", "-c", "SHA256SUMS"],
            cwd=dist,
            check=True,
            capture_output=True,
        )

    def test_missing_manual_prevents_successful_packaging(self):
        (self.root / "man/ripmcp.1").unlink()
        self.assertNotEqual(self.package().returncode, 0)
        self.assertFalse((self.root / "dist/SHA256SUMS").exists())


if __name__ == "__main__":
    unittest.main()
