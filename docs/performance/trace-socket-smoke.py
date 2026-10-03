#!/usr/bin/env python3
"""Observe original smoke phases without modifying its behavior or assertions."""
import importlib.util
import json
from pathlib import Path
import sys
import time

smoke_path, binary, output = map(Path, sys.argv[1:])
spec = importlib.util.spec_from_file_location("measured_smoke", smoke_path)
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)
phases = []


def observe(name, function):
    def measured(*args, **kwargs):
        started = time.monotonic()
        try:
            return function(*args, **kwargs)
        finally:
            phases.append({"phase": name, "seconds": time.monotonic() - started})
    return measured


smoke.App.__init__ = observe("process spawn through listening readiness", smoke.App.__init__)
smoke.App.close = observe("SIGINT through process exit and output drain", smoke.App.close)
original_wait = smoke.wait_for_job


def wait_for_job(app, job_id, status, timeout=4):
    return observe(f"poll job {job_id} until {status}", original_wait)(app, job_id, status, timeout)


smoke.wait_for_job = wait_for_job
sys.argv = [str(smoke_path), str(binary)]
started = time.monotonic()
smoke.main()
output.write_text(json.dumps({"smoke": str(smoke_path), "binary": str(binary),
                              "seconds": time.monotonic() - started, "phases": phases}, indent=2) + "\n")
