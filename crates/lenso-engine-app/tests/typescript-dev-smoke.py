#!/usr/bin/env python3
"""Activation-confirmed Bun edit-to-request and retained compilation evidence."""
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
    parser.add_argument("--out")
    parser.add_argument("--expect-targeted", action="store_true")
    parser.add_argument("--native-call", action="store_true", help="Use the qualified Native-to-Bun Tool consumer")
    args = parser.parse_args()
    root = Path(args.root).resolve()
    source = root / "app/bun-a/src/plugin.ts"
    original = source.read_text()
    if '"bun-baseline"' not in original:
        raise ValueError("Use the incremental-bun example baseline")
    lines, ports = [], []
    started = time.monotonic()
    child = subprocess.Popen([str(Path(args.cli).resolve()), "app", "dev", "--root", str(root)],
                             stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

    def read():
        for line in child.stdout:
            lines.append(line)
            match = re.search(r"Listening on (http://127\.0\.0\.1:[0-9]+)", line)
            if match:
                ports.append(match[1])

    reader = threading.Thread(target=read, daemon=True)
    reader.start()

    def request(path):
        try:
            with urlopen(ports[-1] + path, timeout=0.3) as response:
                return response.read().decode()
        except (OSError, IndexError):
            return None

    def activations():
        return sum("Watching " in line for line in lines)

    def available(expected):
        if args.native_call:
            results = [line for line in lines if "NATIVE_BUN_RESULT " in line]
            return bool(results) and expected in results[-1]
        return request("/typescript") == expected

    def wait(predicate):
        deadline = time.monotonic() + 240
        while time.monotonic() < deadline:
            if child.poll() is not None:
                raise RuntimeError(f"dev exited: {child.returncode}\n{''.join(lines[-35:])}")
            if predicate():
                return
            feedback = root / ".lenso/dev-feedback.json"
            if not activations() and feedback.is_file():
                report = json.loads(feedback.read_text())
                if report.get("dev_process_id") == child.pid and report.get("status") == "rejected":
                    raise RuntimeError("Initial candidate rejected\n" + "".join(lines[-35:]))
            time.sleep(0.02)
        raise TimeoutError("HTTP activation timed out\n" + "".join(lines[-35:]))

    def proof(path):
        return {"sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "mtime_ns": path.stat().st_mtime_ns}

    def native():
        directory = Path(os.environ.get("CARGO_TARGET_DIR", root / ".lenso/host-cache/source/target")) / "release"
        if not directory.is_dir():
            return {"retained_precompiled_host_origin": proof(Path(args.cli).resolve())}
        paths = [directory / "lenso-generated-local-host", *sorted((directory / "deps").glob("libdevloop*.rlib"))]
        if not all(any(path.name.startswith(prefix) for path in paths)
                   for prefix in ["libdevloop_proof-", "libdevloop_health-"]):
            raise RuntimeError("Expected Proof, Health and generated Host compilation units")
        return {str(path): proof(path) for path in paths}

    def generation():
        candidates = list((root / ".lenso").glob("dev-*/generation-*"))
        return max(candidates, key=lambda path: int(path.name.removeprefix("generation-")))

    def packaging():
        base = generation()
        return {name: hashlib.sha256((base / name).read_bytes()).hexdigest() for name in [
            ".lenso/host", ".lenso/host-mode", "runtime/artifacts/example.bun-a", "runtime/artifacts/example.bun-b",
            "bundles/example.bun-b.lenso-plugin",
        ]}

    result = {}
    try:
        wait(lambda: activations() and available("bun-baseline"))
        result["initial_ready_ms"] = (time.monotonic() - started) * 1000
        result["native_before"] = native()
        result["packaging_before"] = packaging()
        count, ready, started = len(lines), activations(), time.monotonic()
        source.write_text(original.replace('"bun-baseline"', '"bun-edited"'))
        wait(lambda: activations() > ready and available("bun-edited"))
        result["edit_to_request_ready_ms"] = (time.monotonic() - started) * 1000
        result["operation"] = "Native typed ToolProvider.execute" if args.native_call else "HTTP GET /typescript"
        result["request_results"] = [line.strip() for line in lines[count:] if "NATIVE_BUN_RESULT " in line]
        result["unrelated_request_results"] = [line.strip() for line in lines[count:] if "NATIVE_HEALTH_RESULT " in line]
        if args.native_call:
            assert any("unrelated-bun" in line for line in result["unrelated_request_results"])
        result["native_after"] = native()
        result["packaging_after"] = packaging()
        result["feedback"] = json.loads((root / ".lenso/dev-feedback.json").read_text())
        result["compile_lines"] = [line.strip() for line in lines[count:] if "Compiling " in line]
        result["packaging_lines"] = [line.strip() for line in lines[count:] if "Dev packaging Plugin " in line]
        if not args.native_call:
            assert request("/typescript-unrelated") == "unrelated-bun"
        if not args.native_call and (root / "Cargo.toml").is_file():
            assert json.loads(request("/health")) == "ok"
        assert result["native_before"] == result["native_after"], "Unchanged native compilation units rebuilt"
        for name in [".lenso/host", ".lenso/host-mode", "runtime/artifacts/example.bun-b", "bundles/example.bun-b.lenso-plugin"]:
            assert result["packaging_before"][name] == result["packaging_after"][name]
        assert result["packaging_before"]["runtime/artifacts/example.bun-a"] != result["packaging_after"]["runtime/artifacts/example.bun-a"]
        if args.expect_targeted:
            assert result["feedback"]["host_build_invoked"] is False
            assert result["feedback"]["packaged_plugins"] == ["example.bun-a"]
            assert len(result["packaging_lines"]) == 1
            if "affected_instances" in result["feedback"]:
                assert result["feedback"]["affected_instances"] == ["example.bun-a/default"]
                assert result["feedback"]["activation_scope"] == "host_generation"
    finally:
        child.send_signal(signal.SIGINT)
        try:
            child.wait(timeout=30)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
        reader.join(timeout=2)
        source.write_text(original)
        if args.out:
            Path(args.out + ".log").write_text("".join(lines))
    result["shutdown_exit_code"] = child.returncode
    assert child.returncode == 0, "Development lifecycle did not retire cleanly"
    output = json.dumps(result, indent=2)
    if args.out:
        Path(args.out).write_text(output + "\n")
    print(output)


if __name__ == "__main__":
    main()
