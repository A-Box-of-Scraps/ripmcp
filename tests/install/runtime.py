#!/usr/bin/python3
import json
import os
import pathlib
import signal
import subprocess
import sys
import time

root = pathlib.Path(__file__).parent
runtime = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with (root / "events").open("a") as log:
    log.write(json.dumps([runtime, args]) + "\n")
preparing = (
    runtime == "npm"
    or "--package" in args
    or ("--from" in args and "-c" in args)
    or args[0] in ("pull", "image")
)
if preparing:
    (root / "preparing").touch()
    if (root / "prep-pause").exists():
        while not (root / "release").exists():
            time.sleep(0.005)
    if (root / "stall").exists():
        child = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(1000)"], close_fds=False
        )
        (root / "preparation_child").write_text(str(child.pid))
        while True:
            time.sleep(1)
    if (root / "fail").exists():
        print("upstream-secret", file=sys.stderr)
        sys.exit(19)
    if runtime == "npm":
        print(json.dumps("latest" if (root / "bad-version").exists() else "1.2.3"))
    elif runtime == "uvx":
        print("latest" if (root / "bad-version").exists() else "1.2.3")
    elif runtime == "docker" and args[0] == "image":
        print(json.dumps(["fixture@sha256:" + "a" * 64]))
    sys.exit(0)
if runtime == "docker":
    if args[0] == "container":
        if (root / "cleanup-fail").exists():
            sys.exit(1)
        sys.exit(0)
    assert args[0] == "run"
(root / "pid").write_text(str(os.getpid()))
with (root / "launches").open("a") as log:
    log.write(str(os.getpid()) + "\n")
for line in sys.stdin:
    request = json.loads(line)
    method = request["method"]
    with (root / "events").open("a") as log:
        log.write(method + "\n")
    if method == "server/discover":
        result = {
            "resultType": "complete",
            "supportedVersions": ["2026-07-28"],
            "capabilities": {"tools": {}},
        }
    elif method == "tools/list":
        if (root / "verify-fail").exists():
            sys.exit(12)
        if (root / "verify-pause").exists():
            (root / "verifying").touch()
            while not (root / "release").exists():
                time.sleep(0.005)
        if (root / "verify-stall").exists():
            (root / "verifying").touch()
            time.sleep(1000)
        result = {
            "resultType": "complete",
            "tools": [{"name": "echo", "inputSchema": {"type": "object"}}],
        }
    elif method == "tools/call":
        result = {"resultType": "complete", "content": []}
    else:
        continue
    print(
        json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}),
        flush=True,
    )
