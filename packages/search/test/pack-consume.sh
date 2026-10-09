#!/bin/sh
set -eu
: "${SEARCH_TEST_DATABASE_URL:?Use a host-authorized disposable PostgreSQL DB}"
test -f dist/index.js
consumer="$PWD/.pack-consumer"
rm -rf "$consumer"
mkdir -p "$consumer/node_modules/@lenso/search"
bun pm pack --ignore-scripts --filename "$consumer/search.tgz"
tar -xzf "$consumer/search.tgz" --strip-components=1 -C "$consumer/node_modules/@lenso/search"
ln -sfn "$PWD/../lenso" "$consumer/node_modules/@lenso/core"
cp test/consumer/package.json test/consumer/tsconfig.json test/consumer/main.ts "$consumer/"
bun ../../node_modules/typescript/bin/tsc --noEmit -p "$consumer/tsconfig.json"
bun "$consumer/main.ts"
