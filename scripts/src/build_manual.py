import argparse
import subprocess
import sys
import tomllib
from pathlib import Path


PANDOC_VERSION = "3.11"
ROOT = Path(__file__).resolve().parents[2]


def generate(root: Path) -> bytes:
    installed = subprocess.run(
        ["pandoc", "--version"], check=True, capture_output=True, text=True
    ).stdout.splitlines()
    if not installed or installed[0] != f"pandoc {PANDOC_VERSION}":
        raise ValueError(f"Manual generation requires Pandoc {PANDOC_VERSION}")
    with (root / "Cargo.toml").open("rb") as manifest:
        version = tomllib.load(manifest)["package"]["version"]
    return subprocess.run(
        [
            "pandoc",
            "--from=markdown-smart",
            "--to=man",
            "--standalone",
            "--fail-if-warnings",
            "--metadata=title:RIPMCP",
            "--metadata=section:1",
            "--metadata=date:",
            f"--metadata=footer:ripmcp {version}",
            "--metadata=header:User Commands",
            str(root / "docs/reference/manual.md"),
        ],
        check=True,
        stdout=subprocess.PIPE,
    ).stdout


def build(root: Path, check: bool) -> None:
    generated = generate(root)
    destination = root / "man/ripmcp.1"
    if check:
        if not destination.is_file() or destination.read_bytes() != generated:
            raise ValueError(
                "man/ripmcp.1 is stale; run python3 scripts/src/build_manual.py"
            )
    else:
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(generated)


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate the ripmcp manual")
    parser.add_argument("--check", action="store_true", help="Reject stale output")
    args = parser.parse_args()
    try:
        build(ROOT, args.check)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        print(f"Manual generation failed: {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
