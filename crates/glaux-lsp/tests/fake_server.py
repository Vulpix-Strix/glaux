"""A scripted language server for the process tests of glaux-lsp.

    fake_server.py echo                 answers pull diagnostics with "v<version>"
    fake_server.py exit                 exits at once
    fake_server.py silent               reads and never answers
    fake_server.py crash-once <marker>  exits after initialize unless <marker> exists
"""

import json
import os
import sys
import time


def read():
    length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        line = line.strip()
        if not line:
            break
        if line.lower().startswith(b"content-length:"):
            length = int(line.split(b":")[1])
    return json.loads(sys.stdin.buffer.read(length))


def write(message):
    body = json.dumps(message).encode()
    sys.stdout.buffer.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
    sys.stdout.buffer.flush()


mode = sys.argv[1]
if mode == "exit":
    sys.exit(3)
if mode == "silent":
    time.sleep(60)
    sys.exit(0)

versions = {}
while True:
    message = read()
    if message is None:
        break
    method = message.get("method")
    if method == "initialize":
        write({"jsonrpc": "2.0", "id": message["id"],
               "result": {"capabilities": {"diagnosticProvider": {}}}})
    elif method == "initialized" and mode == "crash-once":
        marker = sys.argv[2]
        if not os.path.exists(marker):
            open(marker, "w").close()
            sys.exit(4)
    elif method in ("textDocument/didOpen", "textDocument/didChange"):
        document = message["params"]["textDocument"]
        versions[document["uri"]] = document["version"]
    elif method == "textDocument/didClose":
        versions.pop(message["params"]["textDocument"]["uri"], None)
    elif method == "textDocument/diagnostic":
        uri = message["params"]["textDocument"]["uri"]
        range_ = {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}
        write({"jsonrpc": "2.0", "id": message["id"], "result": {"kind": "full", "items": [
            {"range": range_, "severity": 1, "message": "v%s" % versions.get(uri)}]}})
    elif method == "shutdown":
        write({"jsonrpc": "2.0", "id": message["id"], "result": None})
    elif method == "exit":
        break
