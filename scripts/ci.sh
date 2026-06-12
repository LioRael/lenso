#!/usr/bin/env sh
set -eu

just install-ci
just fmt-check
just rust-check
just test
just generated-check
just arch-check
just sdk-check
