#!/usr/bin/python3
import json
import os
import signal
import subprocess
import sys
import time

if os.environ.get("DOCKER_ROOT"):
    import hashlib
    import pathlib

    engine = pathlib.Path(os.environ["DOCKER_ROOT"])
    if sys.argv[1] == "container":
        if (engine / "fail").exists():
            sys.exit(1)
        label = next(
            arg.removeprefix("label=") for arg in sys.argv if arg.startswith("label=")
        )
        for entry in engine.glob("*.json"):
            container = json.loads(entry.read_text())
            if container["label"] == label:
                print(entry.stem)
        sys.exit(0)
    if sys.argv[1] == "rm":
        (engine / (sys.argv[-1] + ".json")).unlink()
        sys.exit(0)
    if sys.argv[1] == "inspect":
        container = json.loads((engine / (sys.argv[-1] + ".json")).read_text())
        print(
            sys.argv[-1], "/" + container["name"], container["label"].split("=", 1)[1]
        )
        sys.exit(0)
    assert sys.argv[1] == "run"
    name = sys.argv[sys.argv.index("--name") + 1]
    label = sys.argv[sys.argv.index("--label") + 1]
    identity = hashlib.sha256(name.encode()).hexdigest()
    (engine / (identity + ".json")).write_text(
        json.dumps({"name": name, "label": label})
    )

mode, root = sys.argv[-2:]
os.makedirs(root, exist_ok=True)
with open(root + "/pid", "w") as out:
    out.write(str(os.getpid()))
with open(root + "/args", "w") as out:
    json.dump(sys.argv[1:], out)
with open(root + "/launches", "a") as out:
    out.write(str(os.getpid()) + "\n")
if mode == "grandchild":
    child = subprocess.Popen(
        [
            sys.executable,
            "-c",
            "import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(1000)",
        ],
        close_fds=False,
    )
    with open(root + "/grandchild", "w") as out:
        out.write(str(child.pid))
if mode in ("grandchild", "immortal"):
    signal.signal(signal.SIGTERM, signal.SIG_IGN)


def reply(request, result):
    print(
        json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}),
        flush=True,
    )


for line in sys.stdin:
    request = json.loads(line)
    method = request["method"]
    if method == "notifications/cancelled":
        with open(root + "/cancelled", "a") as out:
            out.write(request["params"]["requestId"] + "\n")
        if mode == "cancel_ack":
            reply(
                {"id": request["params"]["requestId"]},
                {"resultType": "complete", "content": []},
            )
        continue
    if method == "server/discover":
        if mode == "stall_init":
            time.sleep(1000)
        reply(
            request,
            {
                "resultType": "complete",
                "supportedVersions": ["2026-07-28"],
                "capabilities": {"tools": {}},
            },
        )
    elif method == "tools/list":
        if os.path.exists(root + "/discovery-wait"):
            open(root + "/discovering", "w").close()
            while os.path.exists(root + "/discovery-wait"):
                time.sleep(0.01)
        tools = [{"name": "echo", "inputSchema": {"type": "object"}}]
        if os.path.exists(root + "/tools.json"):
            with open(root + "/tools.json") as source:
                tools = json.load(source)
        reply(request, {"resultType": "complete", "tools": tools})
    elif method == "tools/call":
        with open(root + "/calls", "a") as out:
            out.write(str(os.getpid()) + "\n")
        if request["params"]["arguments"].get("wait_file"):
            while not os.path.exists(root + "/release"):
                time.sleep(0.01)
        if mode == "crash_call":
            os._exit(19)
        if request["params"]["arguments"].get("stall"):
            continue
        reply(
            request,
            {
                "resultType": "complete",
                "content": [],
                "structuredContent": {
                    "pid": os.getpid(),
                    "mode": mode,
                    "secret": os.environ.get("SECRET"),
                    "arguments": request["params"]["arguments"],
                },
                "isError": mode == "tool_error",
            },
        )
if mode in ("grandchild", "immortal"):
    while True:
        time.sleep(1)
