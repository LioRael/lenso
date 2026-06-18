#!/usr/bin/env sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
console_root=${RUNTIME_CONSOLE_ROOT:-"$repo_root/../lenso-runtime-console"}
dist_dir=${LENSO_CONSOLE_DIST_DIR:-"$repo_root/.lenso/console/dist"}
extensions_dir=${LENSO_CONSOLE_EXTENSIONS_DIR:-"$repo_root/.lenso/console/extensions"}
cli_console_dir=${LENSO_CLI_CONSOLE_DIR:-}

if [ ! -f "$console_root/package.json" ]; then
  echo "Runtime Console repo not found: $console_root" >&2
  echo "Set RUNTIME_CONSOLE_ROOT=/path/to/lenso-runtime-console" >&2
  exit 1
fi

LENSO_CONSOLE_BASE=${LENSO_CONSOLE_BASE:-/console/} pnpm --dir "$console_root" run build

rm -rf "$dist_dir"
mkdir -p "$(dirname -- "$dist_dir")"
cp -R "$console_root/dist" "$dist_dir"
mkdir -p "$extensions_dir"
if [ ! -f "$extensions_dir/registry.json" ]; then
  printf '{"version":1,"bundles":[]}\n' >"$extensions_dir/registry.json"
fi

echo "Runtime Console installed to $dist_dir"

if [ -n "$cli_console_dir" ]; then
  rm -rf "$cli_console_dir/dist" "$cli_console_dir/extensions"
  mkdir -p "$cli_console_dir"
  cp -R "$dist_dir" "$cli_console_dir/dist"
  cp -R "$extensions_dir" "$cli_console_dir/extensions"
  echo "Runtime Console embedded for lenso-cli at $cli_console_dir"
fi
