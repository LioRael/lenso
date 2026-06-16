#!/usr/bin/env sh
set -eu

just fmt-check
just rust-check
just test
just generated-check
just arch-check
