#!/usr/bin/env python3
"""Exercise the documented source Plugin loop through existing CLI operations."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import shlex
import signal
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


REPOSITORY = Path(__file__).resolve().parents[2]


def run(arguments: list[str], timeout: int = 600) -> str:
    print("$ " + shlex.join(arguments), flush=True)
    started = time.monotonic()
    result = subprocess.run(
        arguments, capture_output=True, text=True, timeout=timeout, check=False
    )
    print(f"exit={result.returncode} elapsed={time.monotonic() - started:.2f}s", flush=True)
    if result.returncode:
        raise RuntimeError(result.stderr[-12000:] or result.stdout[-12000:])
    return result.stdout


def request(address: str, body: bytes, expected: int) -> dict:
    request = urllib.request.Request(
        address + "greet", body, {"Content-Type": "application/json"}, method="POST"
    )
    # Loopback smoke does not need a proxy or external service.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        response = opener.open(request, timeout=5)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        payload = json.load(response)
        print(f"POST /greet status={response.status} body={json.dumps(payload)}", flush=True)
        assert response.status == expected, payload
        return payload


def stop(host: subprocess.Popen) -> None:
    if host.poll() is not None:
        return
    os.killpg(host.pid, signal.SIGINT)
    try:
        host.wait(timeout=15)
    except subprocess.TimeoutExpired:
        os.killpg(host.pid, signal.SIGKILL)
        host.wait(timeout=5)
        raise RuntimeError("Host did not stop within 15 seconds")
    assert host.returncode == 0, f"Host stop exit={host.returncode}"
    print(f"Host stop exit={host.returncode}", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=REPOSITORY / "target/debug/lenso")
    args = parser.parse_args()
    cli = str(args.cli.resolve(strict=True))
    if os.name != "posix":
        parser.error("this local process-group smoke requires POSIX")
    with tempfile.TemporaryDirectory(prefix="lenso-agent-flow-") as directory:
        temporary = Path(directory)
        source = temporary / "source"
        run([cli, "app", "create", str(source), "--runtime", "empty"])
        shutil.copytree(
            REPOSITORY / "examples/agent-flow/greeting-source",
            source,
            ignore=shutil.ignore_patterns("target", ".lenso", "dist"),
            dirs_exist_ok=True,
        )
        manifest = source / "Cargo.toml"
        manifest.write_text(
            re.sub(
                r'path = "(../../../crates/[^\"]+)"',
                lambda match: "path = "
                + json.dumps(str(REPOSITORY / "crates" / Path(match[1]).name)),
                manifest.read_text(),
            )
        )
        facts = json.loads(run([cli, "app", "discover", "--root", str(source), "--json"]))
        assert len(facts["candidates"]) == 1, facts
        assert facts["candidates"][0]["plugin_id"] == "example.agent-greeting", facts
        print("discovered=example.agent-greeting evidence=source_metadata_only", flush=True)
        run(["cargo", "test", "--locked", "--manifest-path", str(manifest)])
        distribution = temporary / "dist"
        run([cli, "app", "build", "--root", str(source), "--out", str(distribution)])
        configuration = temporary / "ingress.toml"
        configuration.write_text('bind_address = "127.0.0.1:0"\n')
        run([cli, "plugins", "configure", "lenso.web-ingress", "default",
             "--root", str(distribution), "--file", str(configuration)])
        print(run([cli, "app", "check", "--root", str(distribution)]).strip())
        shown = run([cli, "app", "show", "--root", str(distribution)])
        assert "example.agent-greeting" in shown and "lenso.web-ingress" in shown, shown
        print("show=business Plugin + Web Ingress", flush=True)
        log = temporary / "host.log"
        with log.open("w") as output:
            host = subprocess.Popen(
                [cli, "app", "start", "--from", str(distribution)],
                stdin=subprocess.DEVNULL, stdout=output, stderr=output,
                start_new_session=True,
            )
        try:
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                text = log.read_text()
                match = re.search(r"Listening on (http://127\.0\.0\.1:\d+/)", text)
                if match:
                    address = match[1]
                    print("ready=" + address, flush=True)
                    break
                if host.poll() is not None:
                    raise RuntimeError(f"Host exited before readiness: {text}")
                time.sleep(0.05)
            else:
                raise RuntimeError(f"Host readiness timed out: {log.read_text()}")
            assert request(address, b'{"name":"  Lenso  "}', 200) == {
                "message": "Hello, Lenso!"
            }
            invalid = request(address, b'{"name":"   "}', 400)
            assert invalid["code"] == "invalid_name", invalid
            request(address, b'{"name":', 400)
        finally:
            stop(host)
    print("source Plugin loop passed", flush=True)


if __name__ == "__main__":
    main()
