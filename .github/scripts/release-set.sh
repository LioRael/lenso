#!/usr/bin/env bash
set -euo pipefail

release_set_canonical() {
  local kind="$1"
  local raw="$2"

  jq -e -c --arg kind "$kind" '
    def valid_version:
      type == "string"
      and test("^[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?(\\+[0-9A-Za-z.-]+)?$");
    def valid_name:
      type == "string"
      and (
        if $kind == "cargo" then
          test("^[A-Za-z0-9][A-Za-z0-9_-]*$")
        elif $kind == "npm" then
          test("^@lenso/[A-Za-z0-9][A-Za-z0-9._-]*$")
        else
          false
        end
      );
    . as $set
    | if (
        ($set | type) == "array"
        and ($set | length > 0)
        and ($set | all(.[];
          type == "object"
          and ((keys | sort) == ["package_name", "version"])
          and (.package_name | valid_name)
          and (.version | valid_version)
        ))
        and (($set | map(.package_name) | length)
          == ($set | map(.package_name) | unique | length))
        and (if $kind == "npm" then ($set | length == 1) else true end)
      )
      then ($set | sort_by(.package_name))
      else error("release_set must be a non-empty unique package_name/version array for the selected registry")
      end
  ' <<<"$raw"
}

release_releases_canonical() {
  jq -e -c '
    def valid_name:
      type == "string" and test("^[A-Za-z0-9][A-Za-z0-9_-]*$");
    def valid_version:
      type == "string"
      and test("^[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?(\\+[0-9A-Za-z.-]+)?$");
    . as $releases
    | if (
        ($releases | type) == "array"
        and ($releases | all(.[];
          type == "object"
          and (.package_name | valid_name)
          and (.version | valid_version)
        ))
        and (($releases | map(.package_name) | length)
          == ($releases | map(.package_name) | unique | length))
      )
      then ($releases | map({package_name, version}) | sort_by(.package_name))
      else error("release-plz output must contain unique package_name/version objects")
      end
  ' <<<"$1"
}
