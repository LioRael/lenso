#!/usr/bin/env python3
"""Measure activation-confirmed edit-to-HTTP feedback, outside ordinary gates."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import threading
import time
from urllib.request import urlopen


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True)
    parser.add_argument("--root", required=True)
    parser.add_argument("--rust", action="store_true")
    parser.add_argument("--out", help="Optional JSON evidence file")
    args = parser.parse_args()
    root = Path(args.root).resolve()
    config = root / "plugins/example.greeting/default.toml"
    source = root / "src/lib.rs"
    originals = {path: path.read_text() for path in [config, source]}
    if 'message = "baseline"' not in originals[config]:
        raise ValueError("Use the incremental-dev example with its baseline configuration")
    lines, ports = [], []
    started = time.monotonic()
    child = subprocess.Popen(
        [str(Path(args.cli).resolve()), "app", "dev", "--root", str(root)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )

    def read_lines():
        for line in child.stderr:
            lines.append(line)
            match = re.search(r"Listening on (http://127\.0\.0\.1:[0-9]+)", line)
            if match:
                ports.append(match[1])

    reader = threading.Thread(target=read_lines, daemon=True)
    reader.start()

    def wait(predicate):
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            if child.poll() is not None:
                raise RuntimeError(f"dev exited with {child.returncode}\n{''.join(lines[-40:])}")
            if predicate():
                return
            time.sleep(0.02)
        raise TimeoutError(f"HTTP activation did not complete\n{''.join(lines[-40:])}")

    def greeting(expected):
        if not ports:
            return False
        try:
            with urlopen(ports[-1] + "/greeting", timeout=0.2) as response:
                return json.loads(response.read()) == expected
        except (OSError, ValueError):
            return False

    def activations():
        return sum("Watching " in line for line in lines)

    def artifacts():
        target = os.environ.get("CARGO_TARGET_DIR")
        directory = (Path(target).resolve() if target else root / ".lenso/host-cache/source/target") / "release/deps"
        return {
            str(path.relative_to(root) if path.is_relative_to(root) else path): {
                "mtime_ns": path.stat().st_mtime_ns,
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            }
            for path in directory.glob("libdevloop*.rlib")
        }

    result = {}
    try:
        wait(lambda: activations() and greeting("baseline"))
        result["initial_ready_ms"] = (time.monotonic() - started) * 1000
        before = artifacts()
        if len(before) != 2:
            raise RuntimeError("Expected separate Greeting and Health compilation artifacts")
        result["config_artifacts_before"] = before
        started, count, ready = time.monotonic(), len(lines), activations()
        config.write_text('message = "config-edited"\n')
        wait(lambda: activations() > ready and greeting("config-edited"))
        result["config_edit_to_http_ms"] = (time.monotonic() - started) * 1000
        result["config_artifacts_after"] = artifacts()
        result["config_unchanged_artifacts"] = before == artifacts()
        result["config_compiling_lines"] = [line.strip() for line in lines[count:] if "Compiling " in line]
        feedback = root / ".lenso/dev-feedback.json"
        if feedback.exists():
            result["config_feedback"] = json.loads(feedback.read_text())
        if args.rust:
            started, count, ready = time.monotonic(), len(lines), activations()
            source.write_text(originals[source].replace(
                "self.config.message.clone()", 'format!("{}-implementation", self.config.message)'
            ))
            wait(lambda: activations() > ready and greeting("config-edited-implementation"))
            result["rust_edit_to_http_ms"] = (time.monotonic() - started) * 1000
            result["rust_artifacts_after"] = artifacts()
            result["rust_compiling_lines"] = [line.strip() for line in lines[count:] if "Compiling " in line]
            health = next(path for path in before if "libdevloop_health-" in path)
            if before[health] != result["rust_artifacts_after"][health]:
                raise RuntimeError("Greeting edit rebuilt unrelated Health")
        print(json.dumps(result, indent=2))
        if args.out:
            Path(args.out).write_text(json.dumps(result, indent=2) + "\n")
    finally:
        child.send_signal(signal.SIGINT)
        try:
            child.wait(timeout=20)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
        reader.join(timeout=1)
        for path, contents in originals.items():
            path.write_text(contents)


if __name__ == "__main__":
    main()
