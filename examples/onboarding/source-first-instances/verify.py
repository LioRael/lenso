#!/usr/bin/env python3
"""Exercise built Native/Workers Apps and compare their resolved logical graph."""
import argparse
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import tempfile
import time
import urllib.request


def fetch(url):
    with urllib.request.urlopen(url + "instances", timeout=5) as response:
        assert response.status == 200
        return json.load(response)


def stop(process, sig):
    if process.poll() is None:
        process.send_signal(sig)
    try:
        return process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
        raise AssertionError("Host did not stop within its cleanup budget")


def native(cli, root, temporary):
    log_path = temporary / "native.log"
    with log_path.open("w") as log:
        process = subprocess.Popen(
            [str(cli), "app", "start", "--from", str(root)], stdout=log, stderr=log
        )
        try:
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline and process.poll() is None:
                address = re.search(r"Listening on (http://127\.0\.0\.1:\d+/)", log_path.read_text())
                if address:
                    break
                time.sleep(0.05)
            else:
                raise AssertionError("Native readiness failed: " + log_path.read_text())
            responses = [fetch(address[1]), fetch(address[1])]
            assert responses == [
                ["left:1:hello", "right:1:hello"],
                ["left:2:hello", "right:2:hello"],
            ], responses
            code = stop(process, signal.SIGINT)
            assert code == 0, log_path.read_text()
            return {"responses": responses, "exit_code": code}
        finally:
            stop(process, signal.SIGINT)


def workers(binary, root, temporary):
    modules = ["worker.mjs", "host.js", "host_bg.wasm"]
    modules.extend(f"runtime/{path.name}" for path in sorted((root / "runtime").glob("*.mjs")))
    entries = ",\n".join(
        f'(name={json.dumps(name)}, {"wasm" if name.endswith(".wasm") else "esModule"}'
        f" = embed {json.dumps(os.path.relpath(root / name, temporary))})" for name in modules
    )
    config = temporary / "worker.capnp"
    config.write_text(
        'using Workerd = import "/workerd/workerd.capnp";\n'
        'const config :Workerd.Config = (services = [(name = "main", worker = .main)],'
        'sockets = [(name = "http", address = "127.0.0.1:0", http = (), service = "main")]);\n'
        'const main :Workerd.Worker = (compatibilityDate="2026-09-26",modules=[' + entries + "]);\n"
    )
    log_path = temporary / "workers.log"
    with socket.socket() as listener, log_path.open("w") as log:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        url = f"http://127.0.0.1:{listener.getsockname()[1]}/"
        process = subprocess.Popen(
            [str(binary), "serve", str(config), "--socket-fd", f"http={listener.fileno()}"],
            pass_fds=(listener.fileno(),), stdout=log, stderr=log,
        )
    try:
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            assert process.poll() is None, log_path.read_text()
            try:
                first = fetch(url)
                break
            except OSError:
                time.sleep(0.1)
        else:
            raise AssertionError("Workers readiness failed: " + log_path.read_text())
        responses = [first, fetch(url)]
        assert responses == [["left:1:hello", "right:1:hello"]] * 2, responses
        return {"responses": responses, "exit_code": stop(process, signal.SIGTERM)}
    finally:
        stop(process, signal.SIGTERM)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--workers", type=Path)
    parser.add_argument("--workerd", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if bool(args.workers) != bool(args.workerd):
        parser.error("--workers and --workerd must be supplied together")
    cli, native_root = args.cli.resolve(), args.native.resolve()
    resolution = json.loads(subprocess.check_output([
        str(cli), "app", "show", "--root", str(native_root / "intent"),
        "--host-build", str(native_root / ".lenso/host-build.json"), "--runtime-json",
    ], text=True))
    plan = resolution["plan"]
    assert len(plan["plugin_instances"]) == 4
    assert len(plan["capability_bindings"]) == 3
    with tempfile.TemporaryDirectory(prefix="lenso-source-instances-") as directory:
        temporary = Path(directory)
        result = {"native": native(cli, native_root, temporary)}
        if args.workers:
            worker_root = args.workers.resolve()
            worker_plan = json.loads((worker_root / ".lenso/generated-host/src/plan.json").read_text())
            assert worker_plan == plan, "target lowering changed the resolved logical graph"
            result["workers"] = workers(args.workerd.resolve(), worker_root, temporary)
            result["same_resolved_graph"] = True
    evidence = json.dumps(result, indent=2)
    if args.output:
        args.output.write_text(evidence + "\n")
    print(evidence)


if __name__ == "__main__":
    main()
