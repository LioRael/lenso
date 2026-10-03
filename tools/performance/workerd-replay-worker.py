"""Replay worker. Invoke only through workerd-replay.py watchdog; historical driver stays archived."""
import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
import socket

if not __debug__:
    raise RuntimeError('replay identity checks require Python optimization disabled')

parser = argparse.ArgumentParser()
parser.add_argument('--out', type=Path, required=True)
parser.add_argument('--evidence', type=Path, required=True)
parser.add_argument('--tools', type=Path, required=True)
parser.add_argument('--variants', type=Path, required=True)
parser.add_argument('--port', type=int, default=63739)
args = parser.parse_args()
BASE = args.evidence.resolve()
SETUP = BASE / 'setup'
TOOLS = args.tools.resolve()
CLIENT = SETUP / 'workerd-http-case.mjs'
args.out.mkdir(exist_ok=False)
variants = json.loads((SETUP / 'variants-manifest.json').read_text())
assert variants['variants']['before'].keys() == variants['variants']['after'].keys()
assert [name for name in variants['variants']['before']
        if variants['variants']['before'][name] != variants['variants']['after'][name]] == ['workers-http.mjs']
for variant, files in variants['variants'].items():
    for name, expected in files.items():
        source = args.variants.resolve() / variant / name
        assert source.resolve().is_relative_to(args.variants.resolve()), name
        assert hashlib.sha256(source.read_bytes()).hexdigest() == expected, (variant, name)
qualifications = [BASE / 'corpus-after/status.json', BASE / 'corpus-before/status.json']
for path in qualifications:
    q = json.loads(path.read_text())
    assert q['phase'] == 'qualified' and q['corpus_exit_code'] == 0 and q['slot_released']
    assert not q['remaining_owned_group_rows']
digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
manifest_hash = digest(SETUP / 'variants-manifest.json')
client_hash = digest(CLIENT)
tools_manifest = json.loads((SETUP / 'workerd-tools-manifest.json').read_text())
for item in tools_manifest['files']:
    path = TOOLS / item['file']
    if 'sha256' in item:
        assert digest(path) == item['sha256'], item['file']
    else:
        assert path.is_symlink() and os.readlink(path) == item['symlink'], item['file']
status = dict(schema='lenso.web-workerd-paired.v1', phase='running', started_unix=time.time(),
              pid=os.getpid(), source_sha='58a48391844b2c6fce80bb504232bc25a4630d53',
              variants_manifest_sha256=manifest_hash, client_sha256=client_hash,
              runner_sha256=digest(Path(__file__)), groups=7, runs=[],
              tools_manifest_sha256=digest(SETUP / 'workerd-tools-manifest.json'),
              qualification_receipts=[dict(file=str(p), sha256=digest(p)) for p in qualifications],
              scope='same artifact, only HTTP glue differs; existing five routes; 64KiB; one/eight clients',
              main_budget_seconds=240, total_budget_seconds=300,
              uncertainty='warm closed-loop local HTTP; no confidence interval; warm OS caches; readiness includes Wrangler startup; no server CPU/RSS measurement')
deadline = time.monotonic() + 240
owned = set()
def save():
    temporary = args.out / 'status.pending.json'
    temporary.write_text(json.dumps(status, indent=2) + '\n')
    temporary.replace(args.out / 'status.json')
def group_alive(pgid):
    try:
        os.killpg(pgid, 0)
        return True
    except ProcessLookupError:
        return False

def stop(process):
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(process.pid, sig)
        except ProcessLookupError:
            pass
        until = time.monotonic() + 3
        while group_alive(process.pid) and time.monotonic() < until:
            process.poll()
            time.sleep(.05)
        if not group_alive(process.pid):
            break
    process.poll()
    if group_alive(process.pid):
        raise RuntimeError(f'owned group remains: {process.pid}')

def interrupted(signum, frame):
    raise RuntimeError(f'interrupted by signal {signum}')

signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)
try:
    save()
    for group in range(7):
        for connections in ([1, 8] if group % 2 == 0 else [8, 1]):
            for variant in (['before', 'after'] if group % 2 == 0 else ['after', 'before']):
                assert time.monotonic() < deadline, 'whole matrix deadline'
                assert digest(CLIENT) == client_hash and digest(SETUP / 'variants-manifest.json') == manifest_hash
                label = f'group-{group}-{connections}-{variant}'
                out = args.out / label
                out.mkdir()
                app = out / 'app'
                app.mkdir()
                source = args.variants.resolve() / variant
                for name, expected in variants['variants'][variant].items():
                    assert digest(source / name) == expected, (variant, name)
                    target = app / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(source / name, target)
                env = os.environ.copy()
                env.update(WRANGLER_SEND_METRICS='false', CI='true',
                           WRANGLER_LOG_PATH=str(out / 'wrangler-internal.log'),
                           LENSO_WORKERD_BENCH_URL=f'http://127.0.0.1:{args.port}',
                           LENSO_WORKERD_BENCH_CONNECTIONS=str(connections))
                cmd = ['node', str(TOOLS / 'node_modules/wrangler/bin/wrangler.js'), 'dev', '--local',
                       '--config', str(app / 'wrangler.jsonc'), '--ip', '127.0.0.1', '--port', str(args.port),
                       '--persist-to', str(out / 'local-state'), '--log-level', 'error']
                row = dict(group=group, connections=connections, variant=variant, phase='starting',
                           started_unix=time.time(), command=cmd)
                status['runs'].append(row)
                server = client = None
                with socket.socket() as probe:
                    probe.bind(('127.0.0.1', args.port))
                started = time.monotonic()
                case_deadline = min(deadline, started + 60)
                try:
                    with (out / 'server.stdout').open('x') as stdout, (out / 'server.stderr').open('x') as stderr:
                        server = subprocess.Popen(cmd, cwd=app, env=env, stdout=stdout, stderr=stderr,
                                                  start_new_session=True)
                        owned.add(server.pid)
                        row.update(server_runner_pid=server.pid, server_pgid=server.pid)
                        save()
                        while True:
                            assert server.poll() is None, 'server startup exit'
                            assert time.monotonic() < min(started + 20, case_deadline), 'server startup timeout'
                            try:
                                connection = http.client.HTTPConnection('127.0.0.1', args.port, timeout=1)
                                connection.request('GET', '/method/42')
                                response = connection.getresponse()
                                body = response.read()
                                connection.close()
                                assert response.status == 200 and body == b'GET /method/42'
                                break
                            except (OSError, AssertionError):
                                time.sleep(.1)
                        row['server_start_to_HTTP_ready_seconds'] = time.monotonic() - started
                        with (out / 'case.json').open('x') as output, (out / 'client.stderr').open('x') as error:
                            client = subprocess.Popen(['node', str(CLIENT)], env=env, stdout=output,
                                                      stderr=error, start_new_session=True)
                            owned.add(client.pid)
                            row.update(client_pid=client.pid, client_pgid=client.pid)
                            save()
                            client.wait(timeout=max(.01, case_deadline - time.monotonic()))
                            assert client.returncode == 0, 'client failure'
                        raw = json.loads((out / 'case.json').read_text())
                        assert raw['body_bytes'] == 65536 and raw['connections'] == connections
                        assert raw['requests'] == 1000 and raw['warmup'] == 200 and len(raw['raw']) == 1000
                        for key in ['complete_us', 'first_byte_us']:
                            values = sorted(r[key] for r in raw['raw'])
                            for percentile in [50, 95, 99]:
                                assert raw[key]['p' + str(percentile)] == values[len(values) * percentile // 100]
                        row.update(phase='completed', req_s=raw['req_s'], complete_us=raw['complete_us'],
                                   first_byte_us=raw['first_byte_us'], case_sha256=digest(out / 'case.json'))
                finally:
                    cleanup_errors = []
                    for process in (client, server):
                        if process is not None:
                            try:
                                stop(process)
                            except Exception as error:
                                cleanup_errors.append(str(error))
                    row['cleanup_errors'] = cleanup_errors
                    row.update(finished_unix=time.time(), elapsed_seconds=time.monotonic()-started)
                    save()
                    if cleanup_errors:
                        raise RuntimeError('; '.join(cleanup_errors))
        print(f'completed pair {group+1}/7', flush=True)
    status['phase'] = 'completed'
except Exception as error:
    status.update(phase='failed', error=str(error))
    raise
finally:
    listing = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,pgid=,comm='], text=True)
    status['remaining_owned_group_rows'] = [line for line in listing.splitlines()
                                            if int(line.split()[2]) in owned]
    status.update(finished_unix=time.time(), slot_released=not status['remaining_owned_group_rows'],
                  owned_groups=sorted(owned), valid_cases=sum(r['phase']=='completed' for r in status['runs']))
    save()
    print(json.dumps({k:v for k,v in status.items() if k != 'runs'}, indent=2))
