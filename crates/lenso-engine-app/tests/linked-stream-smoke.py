#!/usr/bin/env python3
"""Opt-in generated source App proof; one stream-only Plugin, two target Hosts.

Run with an already built CLI, pinned JS checkout, wasm-bindgen and Wrangler.
Each stage is separate so the operator can schedule compilation and workerd.
All outputs and processes are owned by the supplied evidence directory.
"""
import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import time

FIRST = b"first\x00\xff"


def available_port():
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args, label, command, cwd=None):
    with (args.out / f"{label}.log").open("wb") as log:
        result = subprocess.run(command, cwd=cwd, stdout=log, stderr=subprocess.STDOUT)
    if result.returncode:
        raise RuntimeError(f"{label} failed: {result.returncode}; see {log.name}")


def prepare(args):
    args.out.mkdir(parents=True, exist_ok=False)
    app = args.out / "source"
    probe = app / "app/probe"
    probe.mkdir(parents=True)
    fixture = Path(__file__).parent / "fixtures/linked-stream-app"
    shutil.copy(fixture / "lib.rs", probe / "lib.rs")
    dependencies = {name: args.core / "crates" / name for name in (
        "lenso", "lenso-kernel", "lenso-capability-http-stream-endpoint",
    )}
    manifest = '[package]\nname="linked-stream-probe"\nversion="0.1.0"\nedition="2024"\n[workspace]\n[lib]\npath="lib.rs"\n[package.metadata.lenso]\nplugin-id="fixture.stream-probe"\nroot-slot="web"\n[dependencies]\nfutures="0.3"\n'
    for name, path in dependencies.items():
        manifest += f'{name}={{path={json.dumps(str(path))}}}\n'
    (probe / "Cargo.toml").write_text(manifest)
    native_port = available_port()
    config = app / "plugins/lenso.web-ingress/default.toml"
    config.parent.mkdir(parents=True)
    config.write_text(f'bind_address="127.0.0.1:{native_port}"\n')
    limits = {"eventLimitMs": 2000, "sessionLimitMs": 300, "cancellationLimitMs": 1000,
              "cleanupTimeoutMs": 1000, "maxConcurrent": 2, "maxResponseBodyBytes": 16}
    (args.out / "limits.json").write_text(json.dumps(limits))
    (args.out / "inputs.json").write_text(json.dumps({
        "native_port": native_port, "workers_port": available_port(),
        "same_plugin_sha256": digest(probe / "lib.rs"),
        "source_core_head": subprocess.check_output(["git", "-C", str(args.core), "rev-parse", "HEAD"], text=True).strip(),
        "fixture_kind": "stream-only; no dummy Request Endpoint",
    }, indent=2))


def build(args, target):
    command = [str(args.cli), "app", "build", "--root", str(args.out / "source"),
               "--out", str(args.out / target)]
    if target == "workers":
        command += ["--target", "workers", "--workers-runtime", str(args.js / "packages/lenso-workers-runtime"),
                    "--wasm-bindgen", str(args.bindgen), "--workers-host-limits", str(args.out / "limits.json")]
    run(args, f"build-{target}", command)
    run(args, f"explain-{target}", [str(args.cli), "app", "explain", "--root", str(args.out / target), "--json"])
    if target == "native":
        run(args, "native-plan", [str(args.cli), "app", "show", "--root", str(args.out / target / "intent"),
                                   "--host-build", str(args.out / target / ".lenso/host-build.json"), "--runtime-json"])


def wait_http(process, port):
    deadline = time.monotonic() + 40
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"Host exited before readiness: {process.returncode}")
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.05)
    raise RuntimeError("Host readiness deadline")


def fetch(port, path):
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    connection.request("GET", path)
    response = connection.getresponse()
    return connection, response


def corpus(port):
    result = []
    failures = {}
    for mode, expected in [("", FIRST + b"second"), ("half-close", FIRST)]:
        connection, response = fetch(port, f"/events?{mode}")
        assert response.status == 200
        assert response.read() == expected
        connection.close()
        result.append(mode or "success")
    connection, response = fetch(port, "/events?hold")
    assert response.status == 200
    assert response.read(len(FIRST)) == FIRST  # Terminal is still pending.
    connection.close()
    result.append("first chunk before terminal + disconnect")
    # A provider RuntimeFailure::PluginFailure invokes Native supervision. With
    # restart=never it must be last; later 503 cannot prove domain termination.
    for mode in ["domain", "fail"]:
        # Native may observe an immediate terminal failure before head flush.
        # The admitted response must fail; unavailable 5xx is never accepted.
        try:
            connection, response = fetch(port, f"/events?{mode}")
        except http.client.RemoteDisconnected:
            failures[mode] = "disconnect before flushed head"
            result.append(mode)
            continue
        assert response.status == 200, (mode, response.status, response.read())
        assert response.read(len(FIRST)) == FIRST
        failed = False
        try:
            response.read()
        except (http.client.IncompleteRead, ConnectionError):
            failed = True
        connection.close()
        assert failed, f"failed terminal became EOF: {mode}"
        failures[mode] = "head/chunk then transport failure"
        result.append(mode)
    return {"cases": result, "failure_transport": failures}


def smoke(args):
    inputs = json.loads((args.out / "inputs.json").read_text())
    fixture = Path(__file__).parent / "fixtures/linked-stream-app"
    workers = args.out / "workers"
    shutil.copy(fixture / "probe.mjs", workers / "probe.mjs")
    config = json.loads((workers / "wrangler.jsonc").read_text())
    config["main"] = "probe.mjs"
    config["dev"]["port"] = inputs["workers_port"]
    (workers / "probe.json").write_text(json.dumps(config))
    commands = {
        "native": [str(args.cli), "app", "start", "--from", str(args.out / "native")],
        "workers": ["node", str(args.wrangler), "dev", "--local", "--config", "probe.json"],
    }
    processes = []
    results = {}
    try:
        for name, command in commands.items():
            log = (args.out / f"host-{name}.log").open("wb")
            process = subprocess.Popen(command, cwd=workers if name == "workers" else args.out,
                                       stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
            processes.append((process, log, name))
            port = inputs[f"{name}_port"]
            wait_http(process, port)
            results[name] = corpus(port)
            if name == "workers":
                connection, response = fetch(port, "/__probe")
                data = response.read()
                assert response.status == 200, data
                results["workerd_session_boundaries"] = json.loads(data)
                connection.close()
            os.killpg(process.pid, signal.SIGINT)
            process.wait(timeout=15)
            log.close()
        assert results["native"]["cases"] == results["workers"]["cases"]
        assert digest(args.out / "source/app/probe/lib.rs") == inputs["same_plugin_sha256"]
        workers_plan = json.loads((workers / ".lenso/generated-host/src/plan.json").read_text())
        native_plan = json.loads((args.out / "native-plan.log").read_text())["plan"]
        assert native_plan == workers_plan, "the two Hosts must execute the same resolved Plan"
        results["same_resolved_plan"] = True
        results["same_plugin_sha256"] = inputs["same_plugin_sha256"]
        results["owned_processes"] = [{"pid": p.pid, "target": n, "exit_code": p.returncode} for p, _, n in processes]
        (args.out / "RESULT.json").write_text(json.dumps(results, indent=2))
        print(json.dumps(results, indent=2))
    finally:
        for process, log, _ in processes:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=15)
            log.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=["prepare", "dependencies", "native", "workers", "smoke"])
    for name in ["core", "js", "cli", "bindgen", "wrangler", "out"]:
        parser.add_argument(f"--{name}", required=True, type=lambda value: Path(value).resolve())
    args = parser.parse_args()
    if args.stage == "prepare":
        prepare(args)
    elif args.stage == "dependencies":
        run(args, "prepare-lock", ["cargo", "generate-lockfile", "--manifest-path",
                                   str(args.out / "source/app/probe/Cargo.toml")])
    elif args.stage == "smoke":
        smoke(args)
    else:
        build(args, args.stage)
