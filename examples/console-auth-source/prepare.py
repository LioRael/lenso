"""Prepare only task-owned local fixture config using the original Auth operator."""
import argparse, json, os, pathlib, re, subprocess
root = pathlib.Path(__file__).resolve().parent
parser = argparse.ArgumentParser()
parser.add_argument("--auth-source", type=pathlib.Path, required=True)
parser.add_argument("--console-shell", type=pathlib.Path, required=True)
parser.add_argument("--operator", type=pathlib.Path, required=True)
args = parser.parse_args()
source = (args.auth_source / "crates/lenso-auth-api-token-plugin/tests/postgres_auth.rs").read_text()
env = os.environ.copy()
for constant, variable in [("SIGNING_SECRET", "LENSO_AUTH_SIGNING_SECRET"), ("TOKEN_PEPPER", "LENSO_AUTH_TOKEN_PEPPER")]:
    env[variable] = re.search(r'const ' + constant + r': &str = "([^"]+)";', source).group(1)
if not env.get("LENSO_AUTH_DATABASE_URL"):
    raise ValueError("Set the task-owned local fixture database URL")
def operator(*commands):
    return subprocess.check_output([str(args.operator.resolve()), *commands], env=env, text=True).strip()
public_key = operator("public-key")
operator("setup", "source_assembly")
issued = json.loads(operator("issue", "source_assembly", "source-demo", "example.protected:read"))
for directory, key in [("lenso.auth.api-token", "assertion_public_key"), ("example.protected", "public_key")]:
    file = root / "plugins" / directory / "default.toml"
    file.write_text(re.sub(r'^' + key + r' = .*$', key + ' = ' + json.dumps(public_key), file.read_text(), flags=re.M))
file = root / "plugins/lenso.console.web/default.toml"
file.write_text(re.sub(r'^web_root = .*$', 'web_root = ' + json.dumps(str(args.console_shell.resolve())), file.read_text(), flags=re.M))
# A local, ignored fixture file; never emit credential material to logs.
output = root / "local-fixture.json"
with os.fdopen(os.open(output, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), "w") as file:
    file.write(json.dumps({"token": issued["token"], "environment": {key: env[key] for key in ["LENSO_AUTH_DATABASE_URL", "LENSO_AUTH_SIGNING_SECRET", "LENSO_AUTH_TOKEN_PEPPER"]}}))
print("Prepared task-owned local Auth fixture and source Instance config")
