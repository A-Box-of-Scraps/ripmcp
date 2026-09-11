import os
import subprocess
import sys


def validate_ref(environment: dict[str, str]) -> None:
    branch = environment["RELEASE_BRANCH"]
    event = environment["GITHUB_EVENT_NAME"]
    ref = environment["GITHUB_REF"]
    if event == "workflow_dispatch":
        if ref != f"refs/heads/{branch}":
            raise ValueError("Manual releases must run from the default branch")
    elif event != "push" or not ref.startswith("refs/tags/v"):
        raise ValueError("Automatic releases require a v-prefixed tag push")
    result = subprocess.run(
        [
            "git",
            "merge-base",
            "--is-ancestor",
            f"{environment['GITHUB_SHA']}^{{commit}}",
            f"refs/remotes/origin/{branch}",
        ],
        check=False,
    )
    if result.returncode == 1:
        raise ValueError("Release commit must belong to the default branch")
    result.check_returncode()


def main() -> None:
    try:
        validate_ref(dict(os.environ))
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"Release ref validation failed: {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
