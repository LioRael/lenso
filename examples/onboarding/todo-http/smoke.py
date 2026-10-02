#!/usr/bin/env python3
"""Exercise the real NativeWebHost through HTTP with only Python's standard library."""

import contextlib
import http.client
import json
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import tempfile
from urllib.parse import urlsplit


@contextlib.contextmanager
def running_server(binary):
    with tempfile.TemporaryFile(mode="w+") as errors:
        process = subprocess.Popen(
            [binary, "--bind", "127.0.0.1:0"],
            stdout=subprocess.PIPE,
            stderr=errors,
            text=True,
        )
        try:
            with selectors.DefaultSelector() as ready:
                ready.register(process.stdout, selectors.EVENT_READ)
                assert ready.select(timeout=20), "server did not report readiness in 20 seconds"
                line = process.stdout.readline().strip()
            if not line.startswith("LISTENING http://127.0.0.1:"):
                errors.seek(0)
                raise AssertionError(f"server did not become ready: {line}\n{errors.read()}")
            address = urlsplit(line.removeprefix("LISTENING "))
            yield address.hostname, address.port
        finally:
            if process.poll() is None:
                process.send_signal(signal.SIGINT)
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
                    raise AssertionError("server did not shut down within 10 seconds")
            process.stdout.close()
            errors.seek(0)
            assert process.returncode == 0, errors.read()


def request(address, method, path, expected, payload=None, raw=None, content_type="application/json"):
    headers = {}
    body = raw
    if payload is not None:
        body = json.dumps(payload).encode()
    if body is not None:
        headers["Content-Type"] = content_type
    connection = http.client.HTTPConnection(*address, timeout=5)
    try:
        connection.request(method, path, body=body, headers=headers)
        response = connection.getresponse()
        data = response.read()
        assert response.status == expected, (method, path, response.status, expected, data)
        if expected == 204:
            assert data == b"", data
            return None
        if expected in (200, 201):
            assert response.getheader("Content-Type").startswith("application/json")
        if expected in (400, 415):
            assert response.getheader("Content-Type").startswith("application/problem+json")
        return json.loads(data) if data else None
    finally:
        connection.close()


def main(binary):
    with running_server(binary) as address:
        assert request(address, "GET", "/todos", 200) == []
        todo = request(address, "POST", "/todos", 201, {"title": "  Read the docs  "})
        assert todo == {"id": "todo-1", "title": "Read the docs", "completed": False}, todo
        path = f"/todos/{todo['id']}"
        assert request(address, "GET", path, 200) == todo
        assert request(address, "GET", "/todos", 200) == [todo]
        updated = request(address, "PUT", path, 200, {"title": "Ship a todo", "completed": True})
        assert updated == {"id": todo["id"], "title": "Ship a todo", "completed": True}, updated
        assert request(address, "GET", path, 200) == updated

        for invalid in ({"title": " "}, {"title": "x" * 201}):
            problem = request(address, "POST", "/todos", 400, invalid)
            assert problem["code"] == "invalid_title", problem
        problem = request(address, "PUT", path, 400, {"title": "", "completed": False})
        assert problem["code"] == "invalid_title", problem
        request(address, "POST", "/todos", 400, raw=b"{")
        request(address, "POST", "/todos", 400, {"title": "ok", "unexpected": True})
        request(address, "POST", "/todos", 415, raw=b"hello", content_type="text/plain")
        assert request(address, "GET", path, 200) == updated
        assert request(address, "GET", "/todos", 200) == [updated]
        request(address, "PATCH", path, 405)
        request(address, "GET", "/does-not-exist", 404)

        for method, payload in (("GET", None), ("PUT", {"title": "missing", "completed": False}), ("DELETE", None)):
            problem = request(address, method, "/todos/missing", 404, payload)
            assert problem["code"] == "todo_not_found", problem
        request(address, "DELETE", path, 204)
        request(address, "GET", path, 404)
        request(address, "DELETE", path, 404)
        assert request(address, "GET", "/todos", 200) == []
        second = request(address, "POST", "/todos", 201, {"title": "Transient"})
        assert second["id"] == "todo-2", second

    with running_server(binary) as address:
        assert request(address, "GET", "/todos", 200) == []
    print("PASS: real HTTP CRUD, validation, malformed input, routing errors, graceful shutdown, fresh restart")


if __name__ == "__main__":
    if len(sys.argv) > 2:
        raise SystemExit("usage: smoke.py [/path/to/lenso-onboarding-todo-http]")
    default_binary = Path(__file__).resolve().parents[3] / "target/debug/lenso-onboarding-todo-http"
    main(sys.argv[1] if len(sys.argv) == 2 else str(default_binary))
