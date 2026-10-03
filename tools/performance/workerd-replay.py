"""Offline verification by default; explicitly requested replay uses an independent watchdog."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import statistics
import subprocess
import sys
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_text())


def check(condition, message):
    if not condition:
        raise ValueError(message)


def same(actual, expected, label):
    check(math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-9), label)


def verify(docs):
    manifest = read(docs / 'web-audit-workerd-canonical-20261003-hashes.json')
    raw = docs / 'web-audit-workerd-canonical-20261003-raw'
    paths = set()
    for row in manifest['files']:
        path = (docs / row['file']).resolve()
        check(path.is_relative_to(raw.resolve()), 'manifest path escapes evidence')
        check(path not in paths, 'duplicate evidence path')
        paths.add(path)
        check(path.stat().st_size == row['bytes'], f'size mismatch: {path}')
        check(digest(path) == row['sha256'], f'hash mismatch: {path}')
    check(paths == {p.resolve() for p in raw.rglob('*') if p.is_file()}, 'unlisted evidence')
    status = read(raw / 'measurement/status.json')
    allocation = read(raw / 'allocation/status.json')
    check(status['phase'] == 'completed' and status['valid_cases'] == 28, 'incomplete matrix')
    check(status['slot_released'] and not status['remaining_owned_group_rows'], 'runner resources')
    check(allocation['slot_released'] and not allocation['remaining_owned_group_rows'], 'allocation resources')
    runs = status['runs']
    expected = [(g, c, v) for g in range(7)
                for c in ([1, 8] if g % 2 == 0 else [8, 1])
                for v in (['before', 'after'] if g % 2 == 0 else ['after', 'before'])]
    check([(r['group'], r['connections'], r['variant']) for r in runs] == expected, 'AB/BA order')
    cases = {}
    for row in runs:
        key = (row['group'], row['connections'], row['variant'])
        path = raw / 'measurement' / f'group-{key[0]}-{key[1]}-{key[2]}' / 'case.json'
        case = read(path)
        check(digest(path) == row['case_sha256'], 'case receipt binding')
        check(row['phase'] == 'completed', 'case failure')
        check(case['body_bytes'] == 65536 and case['requests'] == 1000
              and case['warmup'] == 200 and case['connections'] == key[1]
              and len(case['raw']) == 1000, 'case shape')
        for metric in ('complete_us', 'first_byte_us'):
            values = sorted(r[metric] for r in case['raw'])
            check(all(math.isfinite(x) and x >= 0 for x in values), 'invalid latency')
            for p in (50, 95, 99):
                same(case[metric][f'p{p}'], values[len(values) * p // 100], f'{metric} p{p}')
        same(case['req_s'], row['req_s'], 'throughput receipt')
        same(case['req_s'], 1000 * 1000 / case['elapsed_ms'], 'throughput from elapsed')
        cases[key] = case
    analysis = read(raw / 'measurement/analysis.json')
    for summary in analysis['summary']:
        c = summary['connections']
        for metric, recorded in summary['metrics'].items():
            def value(g, v):
                case = cases[g, c, v]
                if metric == 'req_s':
                    return case['req_s']
                if metric == 'server_start_to_HTTP_ready_ms':
                    row = next(r for r in runs if (r['group'], r['connections'], r['variant']) == (g, c, v))
                    return row['server_start_to_HTTP_ready_seconds'] * 1000
                family, percentile, unit = metric.rsplit('_', 2)
                return case[family + '_us'][percentile] / 1000
            before = [value(g, 'before') for g in range(7)]
            after = [value(g, 'after') for g in range(7)]
            ratios = [a / b for a, b in zip(after, before)]
            for name, values in [('before', before), ('after', after), ('paired_after_over_before', ratios)]:
                for stat, function in [('median', statistics.median), ('min', min), ('max', max)]:
                    same(recorded[name][stat], function(values), f'{c}/{metric}/{name}/{stat}')
            for actual, expected_ratio in zip(recorded['paired_ratios'], ratios):
                same(actual, expected_ratio, 'paired ratio')
    variants = read(raw / 'setup/variants-manifest.json')['variants']
    check(variants['before'].keys() == variants['after'].keys(), 'variant file set')
    check([k for k in variants['before'] if variants['before'][k] != variants['after'][k]]
          == ['workers-http.mjs'], 'variant scope')
    print(f'PASS: {len(paths)} hashes, 28 cases, 28000 samples, all quantiles and paired summaries', flush=True)
    return raw


def processes():
    listing = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,pgid='], text=True, timeout=2)
    return [tuple(map(int, line.split())) for line in listing.splitlines()]


def signal_groups(groups, sig):
    errors = []
    for group in groups:
        try:
            os.killpg(group, sig)
        except ProcessLookupError:
            pass
        except OSError as exc:
            errors.append(f'{group}/{sig}: {exc}')
    return errors


def replay(args, raw):
    check(os.name == 'posix', 'replay requires POSIX process groups')
    check(args.tools and args.variants and args.out, '--replay requires --tools, --variants, --out')
    out = args.out.resolve()
    out.mkdir(exist_ok=False)
    groups = set()
    interrupted = False
    def interrupt(signum, frame):
        nonlocal interrupted
        interrupted = True
    previous = {s: signal.signal(s, interrupt) for s in (signal.SIGINT, signal.SIGTERM)}
    start = time.monotonic()
    worker = None
    error = None
    timed_out = False
    cleanup_errors = []
    try:
        cmd = [sys.executable, str(Path(__file__).with_name('workerd-replay-worker.py')),
               '--evidence', str(raw), '--tools', str(args.tools.resolve()),
               '--variants', str(args.variants.resolve()), '--port', str(args.port),
               '--out', str(out / 'measurement')]
        with (out / 'runner.stdout').open('x') as stdout, (out / 'runner.stderr').open('x') as stderr:
            env = os.environ.copy()
            env.pop('PYTHONOPTIMIZE', None)
            worker = subprocess.Popen(cmd, stdout=stdout, stderr=stderr, env=env, start_new_session=True)
            groups.add(worker.pid)
            while True:
                # Observe descendants even before the worker publishes their process group.
                rows = processes()
                descendants = {worker.pid}
                while True:
                    new = {pid for pid, parent, pgid in rows if parent in descendants}
                    if new <= descendants:
                        break
                    descendants |= new
                groups.update(pgid for pid, parent, pgid in rows if pid in descendants)
                status_path = out / 'measurement/status.json'
                if status_path.exists():
                    try:
                        status = read(status_path)
                        for row in status.get('runs', []):
                            groups.update(row[k] for k in ('server_pgid', 'client_pgid') if k in row)
                    except json.JSONDecodeError:
                        pass
                if worker.poll() is not None:
                    break
                if interrupted or time.monotonic() - start >= 240:
                    timed_out = not interrupted
                    break
                time.sleep(.1)
    except Exception as exc:
        error = str(exc)
    finally:
        # Independent TERM/KILL phases apply even when leaders have already exited.
        for sig in (signal.SIGTERM, signal.SIGKILL):
            cleanup_errors.extend(signal_groups(groups, sig))
            until = min(start + 300, time.monotonic() + 5)
            while time.monotonic() < until:
                if worker is not None:
                    worker.poll()
                try:
                    if not any(pgid in groups for pid, parent, pgid in processes()):
                        break
                except (OSError, subprocess.SubprocessError, ValueError) as exc:
                    cleanup_errors.append(f'cleanup ps: {exc}')
                    break
                time.sleep(.05)
        try:
            remaining = [row for row in processes() if row[2] in groups]
        except (OSError, subprocess.SubprocessError, ValueError) as exc:
            cleanup_errors.append(f'final ps: {exc}')
            remaining = None
        receipt = dict(schema='lenso.workerd-replay-allocation.v1', elapsed_seconds=time.monotonic()-start,
                       main_budget_seconds=240, total_budget_seconds=300,
                       timed_out=timed_out, interrupted=interrupted, error=error,
                       runner_exit_code=worker.poll() if worker else None,
                       owned_groups=sorted(groups), remaining_owned_group_rows=remaining,
                       slot_released=remaining == [], cleanup_errors=cleanup_errors,
                       watchdog_sha256=digest(Path(__file__)),
                       worker_sha256=digest(Path(__file__).with_name('workerd-replay-worker.py')))
        try:
            (out / 'allocation.json').write_text(json.dumps(receipt, indent=2) + '\n')
        finally:
            for sig, handler in previous.items():
                signal.signal(sig, handler)
    check(remaining == [] and not cleanup_errors and not timed_out and not interrupted and error is None
          and receipt['runner_exit_code'] == 0, f'replay failed; see {out / "allocation.json"}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--docs', type=Path, default=Path(__file__).resolve().parents[2] / 'docs/performance')
    parser.add_argument('--replay', action='store_true', help='launch the existing fixed matrix; requires a resource allocation')
    parser.add_argument('--tools', type=Path)
    parser.add_argument('--variants', type=Path)
    parser.add_argument('--out', type=Path)
    parser.add_argument('--port', type=int, default=63739)
    args = parser.parse_args()
    raw = verify(args.docs.resolve())
    if args.replay:
        replay(args, raw)


if __name__ == '__main__':
    main()
