#!/bin/sh
set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
debt_file="$repository_root/scripts/rust-module-size-debt.txt"
file_list=$(mktemp)
trap 'rm -f "$file_list"' EXIT HUP INT TERM

find "$repository_root/crates" "$repository_root/fixtures" \
  -type f -name '*.rs' -print | LC_ALL=C sort > "$file_list"

failed=0
while IFS= read -r source_file; do
  relative_path=${source_file#"$repository_root/"}
  case "$relative_path" in
    */generated.rs|*/generated/*|*/snapshots/*)
      continue
      ;;
  esac

  line_count=$(wc -l < "$source_file" | tr -d ' ')
  line_limit=$(awk -v path="$relative_path" '$1 == path { print $2 }' "$debt_file")
  if [ -z "$line_limit" ]; then
    line_limit=1000
  fi

  if [ "$line_count" -gt "$line_limit" ]; then
    printf '%s\n' \
      "error: $relative_path has $line_count lines (limit: $line_limit)" >&2
    failed=1
  elif [ "$line_count" -gt 600 ]; then
    printf '%s\n' \
      "notice: $relative_path has $line_count lines; review for a cohesive split" >&2
  fi
done < "$file_list"

if [ "$failed" -ne 0 ]; then
  printf '%s\n' \
    'Rust module size check failed. Split by responsibility; do not raise a limit without architecture rationale.' >&2
fi

exit "$failed"
