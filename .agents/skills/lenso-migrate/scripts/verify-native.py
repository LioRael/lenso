#!/usr/bin/env python3
"""Compile/run paired native examples using explicit trusted local source checkouts."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--ts-root', type=Path, required=True)
p.add_argument('--rust-root', type=Path, required=True)
p.add_argument('--bun', default='bun')
p.add_argument('--cargo', default='cargo')
p.add_argument('--output-dir', type=Path, help='New/empty owned directory; otherwise use a temporary directory')
p.add_argument('--offline', action='store_true', help='Resolve Cargo dependencies only from its existing cache')
args = p.parse_args()
ts, rust = args.ts_root.resolve(), args.rust_root.resolve()
assets = Path(__file__).resolve().parent.parent / 'assets'
for path in [ts/'packages/lenso/dist/index.js', ts/'node_modules/typescript/bin/tsc', rust/'crates/plugin-core/Cargo.toml']:
    if not path.is_file():
        p.error('Missing prepared public API/tool: ' + str(path))
owned = None
if args.output_dir:
    out = args.output_dir.resolve()
    if out.exists() and any(out.iterdir()):
        p.error('--output-dir must be absent or empty')
    out.mkdir(parents=True, exist_ok=True)
else:
    owned = tempfile.TemporaryDirectory(prefix='lenso-native-example-')
    out = Path(owned.name)

def run(argv, cwd=out):
    result = subprocess.run(argv, cwd=cwd, text=True, capture_output=True)
    with (out/'commands.log').open('a') as log:
        log.write(json.dumps(argv) + '\n' + result.stdout + result.stderr + '\n')
    if result.returncode:
        raise RuntimeError('Command failed: ' + repr(argv) + '\n' + result.stderr + result.stdout)
    return result.stdout.strip()

try:
    shutil.copy2(assets/'catalog.ts', out/'catalog.ts')
    shutil.copy2(assets/'catalog.rs', out/'catalog.rs')
    modules = out/'node_modules'
    (modules/'@lenso').mkdir(parents=True)
    (modules/'@lenso/core').symlink_to(ts/'packages/lenso', target_is_directory=True)
    (modules/'@types').symlink_to(ts/'node_modules/@types', target_is_directory=True)
    (out/'tsconfig.json').write_text(json.dumps({'compilerOptions': {
        'target':'ES2022', 'module':'Preserve', 'moduleResolution':'Bundler',
        'strict':True, 'noEmit':True, 'skipLibCheck':True, 'types':['bun']
    }, 'files':['catalog.ts']}))
    (out/'Cargo.toml').write_text('''[package]
name = "lenso-skill-native-example"
version = "0.0.0"
edition = "2021"
[workspace]
[[bin]]
name = "catalog"
path = "catalog.rs"
[dependencies]
''' + 'lenso-plugin-core = { path = ' + json.dumps(str(rust/'crates/plugin-core')) + ''' }
tokio = { version = "1", features = ["rt", "macros"] }
serde_json = "1"
''')
    run([args.bun, str(ts/'node_modules/typescript/bin/tsc'), '-p', str(out/'tsconfig.json')])
    cargo = [args.cargo, 'run', '--quiet', '--manifest-path', str(out/'Cargo.toml'),
             '--target-dir', str(out/'target')]
    if args.offline:
        cargo += ['--offline']
    evidence = {}
    for label, flags in [('normal', []), ('rollback', ['--fail'])]:
        ts_result = json.loads(run([args.bun, str(out/'catalog.ts'), *flags]))
        rust_result = json.loads(run([*cargo, '--', *flags]))
        if ts_result.get('values') != rust_result.get('values'):
            raise AssertionError((label, ts_result, rust_result))
        event_key = 'closed' if label == 'normal' else 'rollback'
        if sorted(ts_result[event_key]) != sorted(rust_result[event_key]):
            raise AssertionError((label, ts_result, rust_result))
        evidence[label] = {'typescript': ts_result, 'rust': rust_result}
    evidence['scope'] = 'Native exact instances, successful calls, retained-service close, partial-setup rollback; no network/persistence proof.'
    (out/'result.json').write_text(json.dumps(evidence, indent=2)+'\n')
    print(json.dumps(evidence, indent=2))
    if args.output_dir:
        print('Evidence: ' + str(out))
finally:
    if owned:
        owned.cleanup()
