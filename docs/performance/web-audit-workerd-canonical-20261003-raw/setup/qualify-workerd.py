"""Full existing corpus only; run after a normal retained canonical App build."""
import argparse
import hashlib
import http.client
import json
import os
from pathlib import Path
import signal
import subprocess
import time

BASE = Path('/tmp/lenso-web-performance-20261002')
ROOT = Path('/Users/leosouthey/Projects/framework/.worktrees/lenso/codex-web-performance-audit-20261002')
TOOLS = BASE / 'workerd-tools-frozen-20261003'
parser = argparse.ArgumentParser()
parser.add_argument('--app', type=Path, required=True)
parser.add_argument('--out', type=Path, required=True)
parser.add_argument('--expected-glue-sha256', default='2bfe1b31462cc97b513a51c3177640311b4a6791b7582372b0e31a7439a66609',
                    choices=['2bfe1b31462cc97b513a51c3177640311b4a6791b7582372b0e31a7439a66609',
                             'ce552f355dd08daed61bcd73418cf784407aeb0f4547c2315432013fd394ccd2'])
args = parser.parse_args()
args.app = args.app.resolve(strict=True)
args.out.mkdir(exist_ok=False)
receipt = json.loads((args.app / 'workers-build.json').read_text())
assert receipt['component_digest'] == 'sha256:d66a8655f6c54923cb0be0bae8dbab4eb1dcb2a8507cf6b238447464662e1afe'
assert receipt['jco_version'] == '1.35.0'
assert receipt['workers_runtime']['version'] == '0.1.5'
assert hashlib.sha256((args.app / 'workers-http.mjs').read_bytes()).hexdigest() == args.expected_glue_sha256
env = os.environ.copy()
env.update(WRANGLER_SEND_METRICS='false', WRANGLER_LOG_PATH=str(args.out / 'wrangler-internal.log'), CI='true')
cmd = ['node', str(TOOLS / 'node_modules/wrangler/bin/wrangler.js'), 'dev', '--local',
       '--config', str(args.app / 'wrangler.jsonc'), '--ip', '127.0.0.1', '--port', '63739',
       '--persist-to', str(args.out / 'local-state'), '--log-level', 'error']
status = dict(phase='starting', started_unix=time.time(), command=cmd, build_receipt=receipt,
              measurement_started=False, glue_sha256=args.expected_glue_sha256)
process = None
corpus = None
try:
    with (args.out / 'server.stdout').open('x') as stdout, (args.out / 'server.stderr').open('x') as stderr:
        process = subprocess.Popen(cmd, cwd=args.app, env=env, stdout=stdout, stderr=stderr,
                                   start_new_session=True)
        status.update(pid=process.pid, pgid=process.pid)
        (args.out / 'status.json').write_text(json.dumps(status, indent=2))
        deadline = time.monotonic() + 30
        while True:
            assert process.poll() is None, 'server exited before readiness'
            try:
                connection = http.client.HTTPConnection('127.0.0.1', 63739, timeout=1)
                connection.request('GET', '/method/42')
                response = connection.getresponse()
                body = response.read()
                connection.close()
                assert response.status == 200 and body == b'GET /method/42'
                break
            except (OSError, AssertionError):
                assert time.monotonic() < deadline, 'bounded startup readiness timeout'
                time.sleep(.1)
        env.update(LENSO_HTTP_APP_URL='http://127.0.0.1:63739',
                   LENSO_HTTP_APP_ENVIRONMENT='local-workerd', LENSO_HTTP_CORPUS='full')
        t = time.monotonic()
        corpus = subprocess.Popen(['node', str(ROOT / 'crates/lenso-engine-app/tests/workers-app-smoke.mjs')],
                                  env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        result, error = corpus.communicate(timeout=45)
        (args.out / 'corpus.json').write_text(result)
        (args.out / 'corpus.stderr').write_text(error)
        status.update(corpus_pid=corpus.pid, corpus_exit_code=corpus.returncode,
                      corpus_seconds=time.monotonic()-t)
        assert corpus.returncode == 0, error
        raw = json.loads(result)
        assert raw['passed'] and len(raw['results']) == 10 and all(r['passed'] for r in raw['results'])
        status['phase'] = 'qualified'
except Exception as error:
    status.update(phase='failed', error=str(error))
    raise
finally:
    if corpus is not None and corpus.poll() is None:
        corpus.terminate()
        try:
            corpus.wait(timeout=2)
        except subprocess.TimeoutExpired:
            corpus.kill()
            corpus.wait(timeout=2)
    if process is not None:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
        listing = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,pgid=,comm='], text=True)
        status['remaining_owned_group_rows'] = [line for line in listing.splitlines()
                                                if int(line.split()[2]) == process.pid]
        status['slot_released'] = not status['remaining_owned_group_rows']
    status.update(finished_unix=time.time(), elapsed_seconds=time.time()-status['started_unix'])
    (args.out / 'status.json').write_text(json.dumps(status, indent=2) + '\n')
    print(json.dumps({k: v for k, v in status.items() if k != 'build_receipt'}, indent=2))
