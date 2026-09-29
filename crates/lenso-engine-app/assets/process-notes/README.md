The root Process Plugin provides `POST /notes` and `GET /notes/{id}`. Notes are
in-memory development data and do not survive a restart. Process Plugins are
trusted native executables, not sandboxed.

Edit `src/lib.rs` to change the application. Each handler declares its route
once with `#[post]` or `#[get]`; `Json`, `Path`, and typed return values handle
the HTTP mapping. The SDK generates the Endpoint description and Process
dispatch. Run `cargo test` for the generated business tests, then `lenso dev`
to exercise the real HTTP listener. Editing a handler rebuilds the Guest while
reusing the precompiled Host.

The typed Process SDK and matching CLI are local release candidates,
not proof of a published SDK. This App selected its SDK source explicitly with
`--http-sdk-source`. Creation checks the selected SDK and compiles the starter;
`--no-install` skips dependency installation and compilation only.

Use the same CLI binary for build and start. This SDK path selection is source
verification, not a registry-only distribution claim. See the
[CLI installation status](https://github.com/LioRael/lenso#try-one-plugin)
before choosing released packages instead.

To remove the Web surface from a built App, disable both
`local.starter/default` and `lenso.web-ingress/default` under the built Plugin
Root, then run `lenso app start --from dist --root dist`. Ingress without
Endpoint routes refuses readiness. Plain `--from dist` keeps the immutable
build snapshot.
