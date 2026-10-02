#!/usr/bin/env python3
"""Exercise a real native Host using only invented file metadata."""
import contextlib
import json
import pathlib
import selectors
import signal
import subprocess
import sys
import urllib.error
import urllib.request


ROOT = pathlib.Path(__file__).resolve().parents[3]
BINARY = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else (
    ROOT / "target/debug/lenso-onboarding-metadata-pipeline"
)
HTTP = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def inspect(*arguments):
    result = subprocess.run(
        [str(BINARY), "--inspect", *arguments],
        capture_output=True, text=True, timeout=15, check=True,
    )
    return json.loads(result.stdout)


@contextlib.contextmanager
def running(*arguments):
    process = subprocess.Popen(
        [str(BINARY), *arguments], stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, text=True,
    )
    try:
        with selectors.DefaultSelector() as reader:
            reader.register(process.stdout, selectors.EVENT_READ)
            if not reader.select(timeout=15):
                raise AssertionError("Host did not become ready within 15 seconds")
            line = process.stdout.readline().strip()
        assert line.startswith("LISTENING http://127.0.0.1:"), line
        yield line.removeprefix("LISTENING ")
    finally:
        if process.poll() is None:
            process.send_signal(signal.SIGINT)
        try:
            output, _ = process.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            output, _ = process.communicate(timeout=5)
            raise AssertionError("Host failed to shut down cleanly")
        assert process.returncode == 0, (process.returncode, output)


def post(base, files):
    request = urllib.request.Request(
        base + "/metadata/summary",
        data=json.dumps({"files": files}).encode(),
        headers={"Content-Type": "application/json"}, method="POST",
    )
    try:
        response = HTTP.open(request, timeout=5)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        body = response.read().decode()
        return response.status, json.loads(body) if body else None


def main():
    plan = inspect()
    serialized = json.dumps(plan)
    assert "example.metadata-normalizer" in serialized, plan
    assert "example.metadata-summary" in serialized, plan
    assert "example.metadata@1" in serialized, plan
    assert any(
        binding["capability_id"] == "example.metadata@1"
        and binding["consumer_instance"] == "example.metadata-summary/default"
        and binding["provider_instance"] == "example.metadata-normalizer/default"
        for binding in plan["capability_bindings"]
    ), plan
    print(json.dumps({"phase": "resolved-composition", "plan": plan}), flush=True)

    fixtures = [
        {"name": " report.PDF ", "size_bytes": 1024},
        {"name": "notes.txt", "size_bytes": 512},
        {"name": "image.PNG", "size_bytes": 4096},
    ]
    with running() as base:
        status, result = post(base, fixtures)
        assert status == 200, (status, result)
        assert result == {
            "files": [
                {"name": "report.PDF", "extension": "pdf", "size_bytes": 1024},
                {"name": "notes.txt", "extension": "txt", "size_bytes": 512},
                {"name": "image.PNG", "extension": "png", "size_bytes": 4096},
            ],
            "count": 3, "total_bytes": 5632,
            "by_extension": {"pdf": 1, "txt": 1, "png": 1},
        }, result
        print(json.dumps({"phase": "success", "status": status, "result": result}), flush=True)

        for invalid in [
            {"name": " ", "size_bytes": 1},
            {"name": "../secret.txt", "size_bytes": 1},
            {"name": "valid.txt", "size_bytes": -1},
        ]:
            status, problem = post(base, [invalid])
            assert status == 422, (status, problem)
            assert "invalid_metadata" in json.dumps(problem), problem
            print(json.dumps({"phase": "domain-rejection", "status": status, "problem": problem}), flush=True)

        status, result = post(base, [])
        assert status == 200 and result["count"] == 0 and result["total_bytes"] == 0
        status, problem = post(base, [fixtures[0]] * 101)
        assert status == 400 and problem["code"] == "batch_too_large", (status, problem)
        print(json.dumps({"phase": "batch-limit", "status": status, "problem": problem}), flush=True)

    missing = subprocess.run(
        [str(BINARY), "--without-normalizer"],
        capture_output=True, text=True, timeout=15,
    )
    assert missing.returncode != 0, missing.stdout
    assert "LISTENING" not in missing.stdout, missing.stdout
    assert "example.metadata@1" in missing.stderr, missing.stderr
    print(json.dumps({
        "phase": "missing-dependency", "exit_code": missing.returncode,
        "stderr": missing.stderr.strip(),
    }), flush=True)

    removed = inspect("--without-summary")
    assert "example.metadata-summary" not in json.dumps(removed), removed
    assert "example.metadata-normalizer" in json.dumps(removed), removed
    with running("--without-summary") as base:
        with HTTP.open(base + "/health", timeout=5) as health:
            assert health.status == 200 and json.load(health) == "ok"
        status, result = post(base, fixtures)
        assert status == 404, (status, result)
        print(json.dumps({"phase": "removed-consumer", "status": status, "plan": removed}), flush=True)

    print("PASS: real Plugin call, domain rejection, missing dependency, removal, clean shutdown", flush=True)


if __name__ == "__main__":
    main()
