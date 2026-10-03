#!/usr/bin/env python3
"""Real CLI/Kernel proof: Rust HTTP -> two TS instances -> typed TS dependency."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile

helper_path = Path(__file__).resolve().parent.parent / "source-first-instances/verify.py"
spec = importlib.util.spec_from_file_location("source_instances", helper_path)
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--cli", type=Path, required=True)
parser.add_argument("--native", type=Path, required=True)
parser.add_argument("--workers", type=Path)
parser.add_argument("--workerd", type=Path)
parser.add_argument("--output", type=Path)
args = parser.parse_args()
cli, native = args.cli.resolve(), args.native.resolve()
plan = json.loads(subprocess.check_output([str(cli), "app", "show", "--root", str(native / "intent"),
    "--host-build", str(native / ".lenso/host-build.json"), "--runtime-json"], text=True))["plan"]
assert len(plan["plugin_instances"]) == 4
assert len(plan["capability_bindings"]) == 4
with tempfile.TemporaryDirectory(prefix="lenso-mixed-instances-") as directory:
    temporary = Path(directory)
    result = {"native": helper.native(cli, native, temporary, [
        ["left:1:hello", "right:1:left:2:hello"], ["left:3:hello", "right:2:left:4:hello"]])}
    if args.workers:
        assert args.workerd
        workers = args.workers.resolve()
        target = json.loads((workers / ".lenso/generated-host/src/plan.json").read_text())
        for original, lowered in zip(plan["plugin_instances"], target["plugin_instances"], strict=True):
            if original["execution_class"] == "lenso.bun-process@1":
                assert lowered["execution_class"] == "lenso.workers-js@1"
                assert lowered["runtime_profile"] == "lenso.workers-js-authoring@2"
                for field in ["execution_class", "runtime_profile", "required_target_capabilities", "package_revision"]:
                    lowered.pop(field, None)
                    original.pop(field, None)
        assert target == plan, "target lowering changed the logical graph"
        result["workers"] = helper.workers(args.workerd.resolve(), workers, temporary,
            [["left:1:hello", "right:1:left:2:hello"]] * 2)
        result["same_logical_graph"] = True
print(json.dumps(result, indent=2))
if args.output:
    args.output.write_text(json.dumps(result, indent=2) + "\n")
