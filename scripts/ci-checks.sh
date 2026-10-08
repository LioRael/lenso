#!/usr/bin/env bash
set -euo pipefail

unset LENSO_TEST_DATABASE_URL TASK_TEST_DATABASE_URL SCHEDULER_TEST_DATABASE_URL DATABASE_URL
unset AUTHORIZATION_TEST_PG_URL AUTHORIZATION_TEST_PG_OWNED
# These checks create and stop only their own cluster, never a shared service.
for binary in initdb pg_ctl postgres createdb; do
  command -v "$binary" >/dev/null || { echo "Required PostgreSQL binary missing: $binary" >&2; exit 1; }
done
root=$(mktemp -d "${TMPDIR:-/tmp}/lenso-ci-pg.XXXXXXXX")
cleanup() {
  if [[ -f "$root/data/postmaster.pid" ]]; then
    pg_ctl -D "$root/data" -w -m immediate stop
  fi
  rm -rf "$root"
}
trap cleanup EXIT
initdb -D "$root/data" -U lenso_ci --auth=trust --no-locale --encoding=UTF8
port=$(bun -e 'const s=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response()}); console.log(s.port); s.stop(true)')
pg_ctl -D "$root/data" -l "$root/postgres.log" -w -t 15 \
  -o "-h 127.0.0.1 -p $port -k '' -c fsync=off" start
for database in notes_ci tasks_package_ci tasks_example_ci scheduler_ci authorization_fixture; do
  createdb -h 127.0.0.1 -p "$port" -U lenso_ci "$database"
done

bun run lint
bun run fmt:check
bun run build
bun run typecheck
LENSO_REQUIRE_POSTGRES=1 bun run test
LENSO_TEST_DATABASE_URL="postgres://lenso_ci@127.0.0.1:$port/notes_ci" \
  bun test examples/notes/test/postgres.test.ts
TASK_TEST_DATABASE_URL="postgres://lenso_ci@127.0.0.1:$port/tasks_package_ci" \
  bun test packages/tasks/test/postgres.test.ts
SCHEDULER_TEST_DATABASE_URL="postgres://lenso_ci@127.0.0.1:$port/scheduler_ci" \
  bun test packages/scheduler/test/postgres.test.ts
AUTHORIZATION_TEST_PG_URL="postgres://lenso_ci@127.0.0.1:$port/authorization_fixture" \
  AUTHORIZATION_TEST_PG_OWNED=1 bun test packages/authorization/test/pg.test.ts
DATABASE_URL="postgres://lenso_ci@127.0.0.1:$port/tasks_example_ci" \
  TASK_QUEUE_NAME=authorization-test bun examples/tasks/src/migrate.ts
TASK_TEST_DATABASE_URL="postgres://lenso_ci@127.0.0.1:$port/tasks_example_ci" \
  bun test examples/tasks/src/postgres.test.ts examples/tasks/src/entry.test.ts examples/tasks/src/telemetry.test.ts
