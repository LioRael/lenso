"""Verify the ordinary CLI App, official Console and original PostgreSQL Auth."""
import argparse, json, os, pathlib, re, signal, subprocess, tempfile, threading, time
import urllib.error, urllib.request
parser = argparse.ArgumentParser()
parser.add_argument('--cli', type=pathlib.Path, required=True)
parser.add_argument('--built', type=pathlib.Path, required=True)
parser.add_argument('--output', type=pathlib.Path, required=True)
args = parser.parse_args()
root = pathlib.Path(__file__).resolve().parent
fixture = json.loads((root / 'local-fixture.json').read_text())
env = dict(os.environ, **fixture['environment'])
lines, urls = [], []
child = subprocess.Popen([str(args.cli.resolve()), 'app', 'start', '--from', str(args.built.resolve())], env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
def read():
    for line in child.stdout:
        lines.append(line)
        match = re.search(r'Listening on (http://127\.0\.0\.1:\d+)', line)
        if match: urls.append(match[1])
reader = threading.Thread(target=read, daemon=True)
reader.start()
def get(path, token=None):
    request = urllib.request.Request(urls[-1] + path, headers={} if token is None else {'Authorization': 'Bearer ' + token})
    try:
        with urllib.request.urlopen(request, timeout=3) as response:
            return response.status, response.read(), response.headers
    except urllib.error.HTTPError as error:
        return error.code, error.read(), error.headers
result = {'composition': 'official Console + original API-token Auth + authored protected HTTP + fixture Secrets + built-in Ingress'}
try:
    deadline = time.monotonic() + 30
    while not urls:
        if child.poll() is not None: raise RuntimeError('App startup failed\n' + ''.join(lines[-30:]))
        if time.monotonic() > deadline: raise TimeoutError('App readiness\n' + ''.join(lines[-30:]))
        time.sleep(.02)
    status, html, _ = get('/')
    assert status == 200 and b'<html' in html and b'/assets/' in html, (status, html[:120])
    asset = re.search(rb'(?:src|href)="(/assets/[^" ]+\.js)"', html).group(1).decode()
    assert get(asset)[0] == 200
    for path in ['/protected', '/api/console/v1/session']:
        assert get(path)[0] == 401, path
        assert get(path, 'invalid-fixture-token')[0] == 401, path
    status, body, _ = get('/protected', fixture['token'])
    assert status == 200 and json.loads(body) == {'subject': 'source-demo'}, (status, body)
    status, body, _ = get('/api/console/v1/session', fixture['token'])
    session = json.loads(body)
    assert status == 200 and session['subject'] == 'source-demo', (status, session)
    result.update(shell_and_asset_http=200, missing_credentials_http=401, invalid_credentials_http=401, protected_http=200, console_session_http=200, subject='source-demo')
finally:
    if child.poll() is None: child.send_signal(signal.SIGINT)
    try: child.wait(timeout=20)
    except subprocess.TimeoutExpired: child.kill(); child.wait()
    reader.join(timeout=1)
    args.output.with_suffix('.log').write_text(''.join(lines))
    result['exit_code'] = child.returncode
assert child.returncode == 0, child.returncode
args.output.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
