# Configuration boundary

Notes Auth instances bind `notesAuthConfig` before setup. Its principals input
accepts the ordinary array or a JSON string; both pass through one principals
schema. `parseNotesPrincipals` remains a defensive direct API with its fixed,
safe error. Callers using the bound plugin should pass the raw JSON, not parse
it before binding. Supplier functions run during source reading, not Auth setup.
`createApplicationAuth` returns that same bound instance.

The schema owns the session defaults and validates their ordering through
Auth's `sessionLifetime`. Worker environment bindings read `NOTES_LOGIN_KEYS`
and optionally `NOTES_RENEW_AFTER_MS`; the D1 database remains a structural
resource, not serialized configuration. Database and store functions are never
config fields.

`lenso.config.ts` declares an explicit, sensitive `NOTES_LOGIN_KEYS` environment
source without reading the secret on import. Database selection is unchanged:
a truthy `DATABASE_URL` and no truthy `SQLITE_PATH` selects PostgreSQL; otherwise
SQLite is selected, using `SQLITE_PATH ?? "output/notes.sqlite"`.

The listener contract owns port 3001. `LENSO_PORT` uses strict environment
number conversion and an integer range of 0 through 65535, including port 0.
Empty, malformed, fractional, and out-of-range values now fail configuration
validation rather than flowing through `Number(...)` to the listener. An
explicit numeric port stays ordinary input. Programmatic callers that omit the
port get the schema default, without implicitly consulting the environment;
the executable entry chooses the environment source explicitly.
