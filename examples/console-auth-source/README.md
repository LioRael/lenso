# Console, original Auth and source HTTP in one App

This Native example consumes the official Console and API-token Auth Plugins as
normal, exact Git dependencies. Their existing root slots and configuration
files select them. An authored source HTTP Plugin declares a dependency on Auth,
verifies its asymmetric assertion and exact operation audience, and returns the
user subject. A small source Secrets Plugin supplies only configured environment
references to the bound Auth Instance. Ingress owns credential extraction.
No custom Host, factory, Descriptor, codec or alternate auth facade is needed.

The selected Console source is the dev-loop candidate `ee75086a`; Auth uses
Console's existing compatible pin `699bd962`. These are source dependencies,
not newly published packages. The published Rust authoring SDK at this boundary
uses the existing explicit `Port<AuthClient>` field form. The current Core
source SDK additionally accepts direct generated clients.

Build the official Console Shell in its own checkout with `pnpm install
--frozen-lockfile` and `pnpm service:web-build`. For a task-owned local PostgreSQL
database, prepare this example using the original Auth operator:

```sh
cargo build --locked --manifest-path /exact/auth-checkout/Cargo.toml \
  -p lenso-auth-api-token-plugin --example api-token-operator
export LENSO_AUTH_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/DATABASE
python3 prepare.py --auth-source /exact/auth-checkout \
  --console-shell /exact/console/apps/shell/dist/client \
  --operator /exact/auth-checkout/target/debug/examples/api-token-operator
lenso app build --out dist \
  --host-many-slot lenso.console.web=lenso.auth@1=auth
python3 verify.py --cli /exact/lenso --built dist --output verification.json
```

Use an empty, dedicated database. `prepare.py` initializes only its
`source_assembly` schema and issues an ephemeral local test token through the
original operator. It reuses Auth's existing committed integration-test signing
material, stores fixture values in an ignored local file, and never prints
credential material. Do not use this preparation helper or fixture Secrets in
production. Supply your existing authorized Secrets Plugin and Auth setup there.

The verifier launches the App through the normal CLI and proves official Shell
and asset HTTP 200, missing/invalid credentials 401, original Auth-backed
`/protected` and `/api/console/v1/session` HTTP 200, and clean shutdown. This is
one composed App, not separate component tests. It does not claim browser
rendering or deployment qualification.

This composition is Native with PostgreSQL and local Shell assets. It is not
qualified for Workers. Workers would require Console-compatible embedded assets,
a compatible Secrets implementation, and Auth's explicit Workers/D1 facilities
and target features. Plugins need not support every target. The mixed source
Native/Workers example is `../onboarding/source-first-mixed`.
