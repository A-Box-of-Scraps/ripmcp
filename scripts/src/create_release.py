import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path


NUMBER = r"(?:0|[1-9][0-9]*)"
IDENTIFIER = r"(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
SEMVER = re.compile(
    rf"{NUMBER}\.{NUMBER}\.{NUMBER}"
    rf"(?:-(?P<prerelease>{IDENTIFIER}(?:\.{IDENTIFIER})*))?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
)


def release_notes(changelog: str) -> str:
    headings = list(re.finditer(r"^## \[Unreleased\][ \t]*$", changelog, re.MULTILINE))
    if len(headings) != 1:
        raise ValueError("Changelog must contain exactly one [Unreleased] section")
    section = changelog[headings[0].end() :]
    notes = re.split(r"^##[ \t]+", section, maxsplit=1, flags=re.MULTILINE)[0].strip()
    if not notes:
        raise ValueError("Changelog [Unreleased] section must not be empty")
    return notes + "\n"


def select_tag(environment: dict[str, str], manifest: Path, override: str) -> str:
    if override:
        return override if override.startswith("v") else f"v{override}"
    if environment["GITHUB_REF_TYPE"] == "tag":
        return environment["GITHUB_REF_NAME"]
    if environment["GITHUB_EVENT_NAME"] == "workflow_dispatch":
        document = tomllib.loads(manifest.read_text())
        return f"v{document['package']['version']}"
    raise ValueError("Automatic releases require a v-prefixed tag")


def validate_tag(tag: str, manifest: Path, override: str) -> re.Match[str] | None:
    version = SEMVER.fullmatch(tag[1:]) if tag.startswith("v") else None
    if not override:
        if version is None:
            raise ValueError(f"Release tag must be v followed by valid SemVer: {tag!r}")
        document = tomllib.loads(manifest.read_text())
        expected = f"v{document['package']['version']}"
        if tag != expected:
            raise ValueError(
                f"Release tag {tag!r} does not match Cargo.toml version: {expected!r}"
            )
    subprocess.run(["git", "check-ref-format", f"refs/tags/{tag}"], check=True)
    return version


def release_command(environment: dict[str, str], manifest: Path) -> list[str]:
    event = environment["GITHUB_EVENT_NAME"]
    if event not in {"push", "workflow_dispatch"}:
        raise ValueError(f"Unsupported release event: {event}")

    manual = event == "workflow_dispatch"
    override = environment.get("RELEASE_VERSION", "") if manual else ""
    is_tag = environment["GITHUB_REF_TYPE"] == "tag"
    tag = select_tag(environment, manifest, override)
    version = validate_tag(tag, manifest, override)

    command = [
        "gh",
        "release",
        "create",
        tag,
        "--repo",
        environment["GITHUB_REPOSITORY"],
        "--title",
        tag,
        "--target",
        environment["GITHUB_SHA"],
        "--notes-file",
        "-",
    ]
    if is_tag and not override:
        command.append("--verify-tag")
    if manual and environment.get("RELEASE_DRAFT") == "true":
        command.append("--draft")
    if version is not None and version.group("prerelease") is not None:
        command.append("--prerelease")
    return command


def main(assets: list[str] | None = None) -> None:
    try:
        command = release_command(dict(os.environ), Path("Cargo.toml"))
        if assets:
            for asset in assets:
                if not Path(asset).is_file():
                    raise ValueError(f"Release asset does not exist: {asset}")
            command.extend(assets)
        notes = release_notes(Path("CHANGELOG.md").read_text())
        notes += (
            f"\n**Full Changelog**: https://github.com/"
            f"{os.environ['GITHUB_REPOSITORY']}/commits/{command[3]}\n"
        )
        subprocess.run(command, input=notes, text=True, check=True)
        if output := os.environ.get("GITHUB_OUTPUT"):
            with Path(output).open("a") as stream:
                stream.write(f"tag={command[3]}\n")
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"Release failed: {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main(sys.argv[1:])
