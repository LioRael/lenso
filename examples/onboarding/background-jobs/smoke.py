#!/usr/bin/env python3
"""Real-socket smoke for the local, in-memory background jobs example."""
from __future__ import annotations

import json
from pathlib import Path
import queue
import signal
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request


class App:
    def __init__(self, binary: Path, *arguments: str):
        self.http = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        self.process = subprocess.Popen(
            [str(binary), *arguments],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        self.lines: list[str] = []
        self.pending: queue.Queue[str] = queue.Queue()
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        deadline = time.monotonic() + 10
        try:
            while time.monotonic() < deadline:
                if self.process.poll() is not None:
                    raise AssertionError(f"Host exited before Ready: {self.lines}")
                try:
                    line = self.pending.get(timeout=0.1)
                except queue.Empty:
                    continue
                if line.startswith("Listening on http://"):
                    self.url = line.removeprefix("Listening on ").strip()
                    return
            raise AssertionError(f"Host readiness timeout: {self.lines}")
        except BaseException:
            self.close()
            raise

    def _read(self):
        assert self.process.stdout is not None
        for line in self.process.stdout:
            self.lines.append(line.strip())
            self.pending.put(line)

    def request(self, path: str, payload=None):
        data = None if payload is None else json.dumps(payload).encode()
        request = urllib.request.Request(
            self.url + path,
            data=data,
            headers={"Content-Type": "application/json"} if data is not None else {},
        )
        try:
            response = self.http.open(request, timeout=3)
        except urllib.error.HTTPError as error:
            response = error
        with response:
            raw = response.read()
            try:
                body = json.loads(raw)
            except json.JSONDecodeError:
                body = raw.decode()
            return response.status, body

    def events(self):
        return [json.loads(line) for line in self.lines if line.startswith('{"event":')]

    def close(self):
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGINT)
        try:
            self.process.wait(timeout=6)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=3)
            raise AssertionError("Host did not stop within its cleanup budget")
        finally:
            self.reader.join(timeout=1)

    def assert_clean(self):
        assert self.process.returncode == 0, self.lines
        assert {"event": "shutdown", "outcome": "clean"} in self.events(), self.lines


def wait_for_job(app: App, job_id: int, status: str, timeout: float = 4):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        code, jobs = app.request("/jobs")
        assert code == 200, (code, jobs)
        job = next(job for job in jobs if job["id"] == job_id)
        if job["status"] == status:
            return job
        time.sleep(0.02)
    raise AssertionError(f"Job {job_id} did not become {status}: {jobs}")


def main():
    repo = Path(__file__).resolve().parents[3]
    binary = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else repo / "target/debug/lenso-onboarding-background-jobs"
    app = App(binary)
    try:
        assert app.request("/health") == (200, "ok")
        assert app.request("/jobs") == (200, [])
        assert app.request("/notifications") == (200, [])
        code, problem = app.request("/jobs", {"message": "", "delay_ms": 50})
        assert code == 400 and problem["code"] == "invalid_job", (code, problem)
        started = time.monotonic()
        code, accepted = app.request("/jobs", {"message": "Local notification", "delay_ms": 1500})
        accepted_ms = round((time.monotonic() - started) * 1000, 1)
        assert code == 202 and accepted["status"] == "queued", (code, accepted)
        assert accepted_ms < 1000, f"submission blocked for {accepted_ms} ms"
        assert app.request("/notifications") == (200, []), "notification appeared before background completion"
        assert app.request("/jobs")[1][0]["status"] in ("queued", "running")
        completed = wait_for_job(app, accepted["id"], "completed")
        assert completed["error"] is None
        assert app.request("/notifications") == (200, [{"job_id": accepted["id"], "message": "Local notification"}])
        print(f"PASS asynchronous acceptance ({accepted_ms} ms), later completion, one in-memory notification")

        code, failed = app.request("/jobs", {"message": "Fail locally", "delay_ms": 50, "fail": True})
        assert code == 202
        failure = wait_for_job(app, failed["id"], "failed")
        assert failure["error"] == "requested_failure"
        assert len(app.request("/notifications")[1]) == 1, "failed job sent a notification"
        print("PASS invalid input and asynchronous failure leave no extra notification")

        code, cancelled = app.request("/jobs", {"message": "Cancel during shutdown", "delay_ms": 5000})
        assert code == 202
        wait_for_job(app, cancelled["id"], "running")
    finally:
        app.close()
    app.assert_clean()
    events = app.events()
    assert {"event": "job_cancelled", "job_id": cancelled["id"]} in events, events
    assert {"event": "plugin_stopped", "new_tasks_rejected": True} in events, events
    dropped = [event["job_id"] for event in events if event["event"] == "task_dropped"]
    assert sorted(dropped) == [accepted["id"], failed["id"], cancelled["id"]], events
    assert not any(event.get("event") == "job_finished" and event.get("job_id") == cancelled["id"] for event in events)
    print("PASS cancellation, every task future dropped once, closed generation rejects new tasks, clean shutdown")
    print(json.dumps({"lifecycle_events": events}, sort_keys=True))

    fresh = App(binary)
    try:
        assert fresh.request("/jobs") == (200, [])
        assert fresh.request("/notifications") == (200, [])
    finally:
        fresh.close()
    fresh.assert_clean()
    print("PASS restart creates fresh in-memory state")

    removed = App(binary, "--without-jobs")
    try:
        assert removed.request("/health") == (200, "ok")
        assert removed.request("/jobs")[0] == 404
        assert removed.request("/notifications")[0] == 404
    finally:
        removed.close()
    removed.assert_clean()
    assert not any(event["event"] == "plugin_stopped" for event in removed.events())
    print("PASS removing the Plugin removes its routes and lifecycle")


if __name__ == "__main__":
    main()
