import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


CHANGELOG = "CHANGELOG.md"


def next_cycle(changelog: str, tag: str, date: str) -> str:
    version = tag.removeprefix("v")
    if not version or any(character in version for character in "\r\n[]"):
        raise ValueError("Release version cannot be used in a changelog heading")
    heading = re.compile(r"^## \[Unreleased\][ \t]*$", re.MULTILINE)
    count = len(heading.findall(changelog))
    released = re.search(
        rf"^## \[{re.escape(version)}\](?:[^\S\n]|$)", changelog, re.MULTILINE
    )
    if released is not None and count <= 1:
        if count == 1:
            return changelog
        return (
            changelog[: released.start()]
            + "## [Unreleased]\n\n"
            + changelog[released.start() :]
        )
    if count != 1:
        raise ValueError("Changelog must contain exactly one [Unreleased] section")
    return heading.sub(
        lambda match: f"## [Unreleased]\n\n## [{version}] - {date}",
        changelog,
        count=1,
    )


def main() -> None:
    try:
        repository = Path("next-cycle")
        changelog = repository / CHANGELOG
        current = changelog.read_text()
        updated = next_cycle(
            current,
            os.environ["RELEASE_TAG"],
            datetime.now(timezone.utc).date().isoformat(),
        )
        if updated == current:
            return
        if current != Path(CHANGELOG).read_text():
            raise ValueError(
                "Default-branch changelog changed since the release commit"
            )
        changelog.write_text(updated)
        commands = [
            ["config", "user.name", "github-actions[bot]"],
            [
                "config",
                "user.email",
                "41898282+github-actions[bot]@users.noreply.github.com",
            ],
            ["add", "--", CHANGELOG],
            ["commit", "-m", "Add [Unreleased] section for next cycle"],
            ["push", "origin", f"HEAD:refs/heads/{os.environ['RELEASE_BRANCH']}"],
        ]
        for command in commands:
            subprocess.run(["git", *command], cwd=repository, check=True)
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"Changelog update failed: {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
