#!/usr/bin/env python3
"""Build and exercise all three local onboarding Apps with bounded processes."""
import argparse
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
EXAMPLES = ("todo-http", "background-jobs", "metadata-pipeline")


def stop_group(process):
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        process.wait()
        return
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        pass
    # The leader may have exited while a descendant ignored SIGTERM.
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


def run(command, timeout, env):
    print(f"+ {shlex.join(map(str, command))}", flush=True)
    process = subprocess.Popen(command, cwd=ROOT, env=env, start_new_session=True)
    try:
        code = process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        print(f"EXIT: 124 (timeout after {timeout}s)", flush=True)
        raise SystemExit(124) from None
    except KeyboardInterrupt:
        print("EXIT: 130 (interrupted)", flush=True)
        raise SystemExit(130) from None
    finally:
        stop_group(process)
    print(f"EXIT: {code}", flush=True)
    if code:
        raise SystemExit(code if code > 0 else 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--jobs", type=int, default=2)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    env = dict(os.environ, CARGO_BUILD_JOBS=str(args.jobs))
    if not args.skip_build:
        command = ["cargo", "build", "--locked"]
        for name in EXAMPLES:
            command += ["-p", f"lenso-onboarding-{name}"]
        run(command, 600, env)
    target = Path(env.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    for name in EXAMPLES:
        binary = target / "debug" / f"lenso-onboarding-{name}"
        run([sys.executable, ROOT / "examples/onboarding" / name / "smoke.py", binary], 90, env)
    print("PASS: three local source Apps; this is not registry or release qualification")


if __name__ == "__main__":
    main()
