#!/usr/bin/env python3
"""Measure the actual CI consumer test with empty and reused child targets."""
import argparse
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import time


def run(command, checkout, log, env):
    started = time.monotonic()
    process = subprocess.Popen(command, cwd=checkout, env=env, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=120)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            stdout, stderr = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate()
        log.write_text(stdout + stderr)
        raise RuntimeError(f"command exceeded deadline: see {log}") from None
    log.write_text(stdout + stderr)
    if process.returncode:
        raise RuntimeError(f"command failed: see {log}")
    return time.monotonic() - started, stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("baseline", "candidate", "output", "build-root"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.build_root = args.build_root.resolve()
    args.build_root.mkdir(parents=True, exist_ok=False)
    args.output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_BUILD_JOBS="2", CARGO_TERM_COLOR="never")
    env.pop("CARGO_TARGET_DIR", None)
    env.pop("LENSO_CARGO_FIXTURE_CACHE_DIR", None)
    assert not env.get("RUSTC_WRAPPER") and not env.get("RUSTC_WORKSPACE_WRAPPER")
    report = {"platform": platform.platform(), "build_jobs": 2,
              "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
              "boundary": "actual child Cargo and assertions; outer harness build excluded; registry sources pre-fetched; no Actions transfer included",
              "cases": {}}
    test = "default_macro_compiles_with_registered_snapshot_and_codegen"
    for label, checkout in [("baseline", args.baseline), ("candidate", args.candidate)]:
        checkout = checkout.resolve()
        assert not subprocess.check_output(["git", "status", "--porcelain"], cwd=checkout), \
            "measure a clean exact source commit"
        build_env = dict(env, CARGO_TARGET_DIR=str(args.build_root / label / "parent"))
        elapsed, messages = run(["cargo", "test", "--locked", "-p",
                                "lenso-contract-authoring-macros", "--test",
                                "registered_snapshot", "--no-run", "--message-format=json"],
                               checkout, args.output / f"{label}-parent-build.txt", build_env)
        artifacts = [json.loads(line) for line in messages.splitlines() if line.startswith("{")]
        binary = next(item["executable"] for item in artifacts
                      if item.get("reason") == "compiler-artifact"
                      and item["target"]["name"] == "registered_snapshot" and item.get("executable"))
        run_env = dict(env)
        if label == "candidate":
            run_env["LENSO_CARGO_FIXTURE_CACHE_DIR"] = str(args.build_root / label / "child")
            assert not Path(run_env["LENSO_CARGO_FIXTURE_CACHE_DIR"]).exists()
        samples = []
        for index in range(2 if label == "baseline" else 4):
            seconds, stdout = run([binary, test, "--exact", "--test-threads=1"], checkout,
                                  args.output / f"{label}-{index}.txt", run_env)
            assert "test result: ok. 1 passed; 0 failed" in stdout, "exact test must execute"
            samples.append(seconds)
            print(json.dumps({"case": label, "sample": index, "seconds": seconds}), flush=True)
        report["cases"][label] = {
            "sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=checkout, text=True).strip(),
            "parent_build_seconds_excluded": elapsed, "process_seconds": samples,
            "child_cache": "fresh per process" if label == "baseline" else "empty first; reused remaining",
        }
        (args.output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
