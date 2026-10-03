#!/usr/bin/env python3
"""Measure the retained socket smoke and its shared-corpus simulation separately."""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import time


def run(command, checkout, log, env):
    started = time.monotonic()
    result = subprocess.run(command, cwd=checkout, env=env, capture_output=True,
                            text=True, timeout=300)
    log.write_text(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError(f"failed command {command!r}: see {log}")
    return time.monotonic() - started, result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--build-root", type=Path, required=True)
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.build_root = args.build_root.resolve()
    # A new directory is required: do not erase or call an existing target cold.
    args.build_root.mkdir(parents=True, exist_ok=False)
    args.output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_BUILD_JOBS="2", CARGO_TERM_COLOR="never")
    report = {"platform": platform.platform(), "build_jobs": 2,
              "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
              "cache_boundary": "empty target per checkout; registry sources pre-fetched; no wrapper environment variables",
              "cases": {}}
    assert not env.get("RUSTC_WRAPPER") and not env.get("RUSTC_WORKSPACE_WRAPPER"), \
        "measure without ambient compiler wrapper environment variables"
    for label, checkout in [("baseline", args.baseline), ("candidate", args.candidate)]:
        checkout = checkout.resolve()
        target = args.build_root / label
        case_env = dict(env, CARGO_TARGET_DIR=str(target))
        build = ["cargo", "test", "--locked", "-p", "lenso-onboarding-background-jobs",
                 "--test", "simulator", "--no-run", "--message-format=json"]
        cold_build, messages = run(build, checkout, args.output / f"{label}-cold-build.log", case_env)
        artifacts = [json.loads(line) for line in messages.splitlines() if line.startswith("{")]
        binary = next(item["executable"] for item in artifacts
                      if item.get("reason") == "compiler-artifact"
                      and item["target"]["name"] == "simulator" and item.get("executable"))
        warm_build, _ = run(build, checkout, args.output / f"{label}-warm-build.log", case_env)
        if label == "baseline":
            command = ["python3", "examples/onboarding/background-jobs/smoke.py",
                       str(target / "debug/lenso-onboarding-background-jobs")]
        else:
            command = [binary, "smoke_corpus_replays_without_wall_clock_waits",
                       "--exact", "--test-threads=1"]
        samples = []
        for index in range(4):
            elapsed, stdout = run(command, checkout, args.output / f"{label}-run-{index}.log", case_env)
            if label == "candidate" and "test result: ok. 1 passed; 0 failed" not in stdout:
                raise RuntimeError("the measured exact simulation test did not execute once")
            samples.append(elapsed)
        report["cases"][label] = {
            "checkout_sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=checkout, text=True).strip(),
            "dirty_files": subprocess.check_output(["git", "status", "--short"], cwd=checkout, text=True).splitlines(),
            "cold_build_seconds": cold_build, "warm_build_seconds": warm_build,
            "first_process_seconds": samples[0], "repeated_process_seconds": samples[1:],
            "command": command,
        }
        if label == "candidate":
            sentinel, stdout = run([binary, "socket_smoke_sentinel_matches_simulated_corpus",
                               "--exact", "--test-threads=1", "--nocapture"], checkout,
                              args.output / "candidate-real-sentinel.log", case_env)
            if "test result: ok. 1 passed; 0 failed" not in stdout:
                raise RuntimeError("the measured exact real sentinel did not execute once")
            report["cases"][label]["real_sentinel_seconds"] = sentinel
        (args.output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({label: report["cases"][label]}), flush=True)


if __name__ == "__main__":
    main()
