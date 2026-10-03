#!/usr/bin/env python3
"""Separate empty-target compilation from real and replay corpus execution."""
import argparse
import json
import os
from pathlib import Path
import signal
import statistics
import subprocess
import time


def invoke(command, environment, log, required_test=None, deadline_seconds=600):
    began = time.perf_counter()
    process = subprocess.Popen(command, env=environment, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=deadline_seconds)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            stdout, stderr = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate(timeout=5)
        log.write_text(stdout + stderr)
        raise RuntimeError(f"bounded command deadline elapsed; see {log}") from None
    elapsed = time.perf_counter() - began
    log.write_text(stdout + stderr)
    if process.returncode:
        raise RuntimeError(f"command failed ({process.returncode}); see {log}")
    if required_test and f"test {required_test} ... ok" not in stdout:
        raise RuntimeError(f"exact filter did not execute {required_test}; see {log}")
    return elapsed, stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    os.chdir(root)
    if args.build_root.exists():
        parser.error("build root must be absent for the empty-target boundary")
    if not os.environ.get("LENSO_STORE_PG_URL"):
        parser.error("LENSO_STORE_PG_URL must name the dedicated real fixture")
    for wrapper in ("RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"):
        if os.environ.get(wrapper):
            parser.error(f"{wrapper} must be unset")
    if subprocess.check_output(["git", "status", "--porcelain"], text=True).strip():
        parser.error("measure a committed, clean source revision")
    args.output.mkdir(parents=True, exist_ok=False)
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = str(args.build_root.resolve())
    environment["CARGO_BUILD_JOBS"] = "2"
    environment.pop("LENSO_STORE_TRACE_OUT", None)
    environment.pop("LENSO_STORE_PROVIDER", None)
    environment.pop("LENSO_STORE_TRANSCRIPT_DIR", None)
    compile_command = ["cargo", "test", "--locked", "-p", "lenso-test", "--test", "durable_store", "--no-run", "--message-format=json"]
    cold, artifacts = invoke(compile_command, environment, args.output / "cold-build.txt")
    paths = [entry["executable"] for line in artifacts.splitlines()
             if (entry := json.loads(line)).get("reason") == "compiler-artifact"
             and entry["target"]["name"] == "durable_store" and entry.get("executable")]
    if len(paths) != 1:
        raise RuntimeError(f"expected one exact test artifact, got {paths}")
    warm, _ = invoke(compile_command, environment, args.output / "warm-build.txt")
    samples = {}
    for mode in ("postgres", "d1", "replay"):
        timings = []
        for index in range(4):
            env = environment.copy()
            test = "replay_provider_store_corpus" if mode == "replay" else "real_provider_store_corpus"
            command = ["timeout", "--kill-after=5s", "90s", paths[0], "--exact", test, "--nocapture"]
            if mode != "replay":
                env["LENSO_STORE_PROVIDER"] = mode
                command.append("--ignored")
            duration, _ = invoke(command, env, args.output / f"{mode}-{index}.txt", test, deadline_seconds=110)
            timings.append(duration)
        samples[mode] = {"first_process_seconds": timings[0], "repeated_process_seconds": timings[1:], "repeated_median_seconds": statistics.median(timings[1:])}
    summary = {"source_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(), "cold_compile_seconds": cold, "warm_compile_seconds": warm, "samples": samples, "comparison": "replay includes both providers' six-case transcripts; each real sample runs one provider's six-case corpus"}
    (args.output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
