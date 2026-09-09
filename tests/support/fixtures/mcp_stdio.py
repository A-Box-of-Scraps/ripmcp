import json
import os
import signal
import sys
import time

mode, root = sys.argv[1:]
with open(root + "/pid", "w") as handle:
    handle.write(str(os.getpid()))


def response(request, result):
    raw = (
        json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}) + "\n"
    ).encode()
    size = 3 if mode == "fragmented" else len(raw)
    for offset in range(0, len(raw), size):
        os.write(1, raw[offset : offset + size])


def error(request):
    print(
        json.dumps(
            {
                "jsonrpc": "2.0",
                "id": request["id"],
                "error": {"code": -32603, "message": "secret-upstream-error"},
            }
        ),
        flush=True,
    )


def tool(name):
    return {
        "name": name,
        "description": "fixture",
        "inputSchema": {"type": "object"},
        "vendor": {"retained": True},
    }


def result(request):
    return {
        "resultType": "complete",
        "content": [
            {"type": "text", "text": "fixture", "vendor": 1},
            {"type": "image", "data": "AQI=", "mimeType": "image/png"},
            {"type": "audio", "data": "AwQ=", "mimeType": "audio/wav"},
            {"type": "resource_link", "name": "link", "uri": "fixture:///link"},
            {
                "type": "resource",
                "resource": {"uri": "fixture:///data", "blob": "BQY="},
            },
        ],
        "structuredContent": request["params"].get("arguments", {}),
        "isError": mode == "tool_error",
        "_meta": {"fixture": True},
        "extension": {"large": 1234567890123456789012345678901234567890},
    }


pending = []
calls = 0
if mode == "stderr_flood":
    os.write(2, b"secret-server-stderr" * 65536)
else:
    os.write(2, b"secret-server-stderr\n")
if mode == "immortal":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
for line in sys.stdin:
    request = json.loads(line)
    with open(root + "/requests", "a") as handle:
        handle.write(json.dumps(request) + "\n")
    method = request["method"]
    if method == "notifications/cancelled":
        with open(root + "/cancelled", "w") as handle:
            handle.write(request["params"]["requestId"])
        for old in pending:
            response(old, result(old))
        pending = []
        continue
    assert (
        request["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"]
        == "2026-07-28"
    )
    assert (
        request["params"]["_meta"]["io.modelcontextprotocol/clientCapabilities"] == {}
    )
    if method == "server/discover":
        if mode == "stall_init":
            time.sleep(10)
            continue
        if mode == "malformed":
            print("not-json", flush=True)
            continue
        if mode == "oversize":
            os.write(1, b"x" * 4096)
            continue
        if mode == "exit":
            sys.exit(17)
        if mode == "unterminated":
            os.write(1, b'{"jsonrpc":"2.0"}')
            sys.exit(0)
        if mode == "server_request":
            print(
                json.dumps(
                    {
                        "jsonrpc": "2.0",
                        "id": "server-1",
                        "method": "sampling/createMessage",
                        "params": {},
                    }
                ),
                flush=True,
            )
            continue
        if mode == "wrong_id":
            request["id"] = "ripmcp-99999"
        response(
            request,
            {
                "resultType": "complete",
                "supportedVersions": [
                    "2025-11-25" if mode == "version" else "2026-07-28"
                ],
                "capabilities": {} if mode == "no_tools" else {"tools": {}},
                "vendor": "retained",
            },
        )
    elif method == "tools/list":
        cursor = request["params"].get("cursor")
        if mode == "slow_discovery":
            time.sleep(0.07)
        if mode == "list_failure" and cursor:
            error(request)
            continue
        page = {
            "resultType": "complete",
            "tools": [tool("echo2" if cursor else "echo")],
        }
        if mode == "cycle" or (
            not cursor and mode not in ["concurrent", "cancel", "drop_call"]
        ):
            page["nextCursor"] = "page2"
        response(request, page)
    elif method == "tools/call":
        calls += 1
        with open(root + "/called", "w") as handle:
            handle.write(str(calls))
        if mode == "drop_call":
            sys.exit(19)
        if mode == "cancel" and calls == 1:
            pending.append(request)
            continue
        if mode == "concurrent":
            pending.append(request)
            if len(pending) == 2:
                for queued in reversed(pending):
                    response(queued, result(queued))
                pending = []
            continue
        if mode == "input_required":
            response(
                request,
                {
                    "resultType": "input_required",
                    "inputRequests": [],
                    "requestState": "private",
                },
            )
        elif mode == "protocol_error":
            error(request)
        else:
            response(request, result(request))
    else:
        raise AssertionError("unexpected method " + method)
if mode == "immortal":
    while True:
        time.sleep(1)
