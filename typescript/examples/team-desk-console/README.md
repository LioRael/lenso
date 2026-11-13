# Team Desk Console host

An independent browser host for Team Desk. One set of compiled pages connects to either the native TypeScript application or the native Rust application through `@lenso/console`. The host is a separate process and package; neither backend serves or imports its UI.

## Start the complete local example

From the repository root:

```sh
npm ci
npm run build
cargo build --manifest-path rust/Cargo.toml --workspace
npm run team-console
```

Open `http://127.0.0.1:5173`. The launcher starts both backends on available loopback ports and the built Console host on port 5173. It uses the public demo identities from `examples/team-desk/identities.example.json` and separate persistent databases in `.local/team-console/`. Stop it with Ctrl+C to stop all three processes. Set `TEAM_CONSOLE_PORT` or `TEAM_CONSOLE_DATA` to change the port or data directory. The launcher does not overwrite the earlier standalone Team Desk databases.

Choose a backend and account, then click **Connect**. Tokens stay in this tab's memory, are sent in the Authorization header, and are never stored in local/session storage or put in a URL. The role displayed in the sidebar comes from the selected application's `members.session` Action, not from the account selector.

To run the host against separately started applications, set `TEAM_DESK_TS_URL` and `TEAM_DESK_RUST_URL`, then run `npm run start --workspace @lenso/example-team-console`. Defaults are `http://127.0.0.1:3000` and `http://127.0.0.1:3001`. Both development and built-preview modes use fixed configured same-origin proxy routes; a browser request cannot supply a new proxy target. A deployed host would need an equivalent proxy configured by its operator. Vite preview is a local reference server, not a production hosting recommendation.

## Read/write workflow

1. As Alice, open **Projects**, create an ID/name, then open **Work items** and create an item assigned to Bob.
2. Select the item. Change its assignment and advance `Open -> In progress -> Done`. The editor displays the server revision used by the next write.
3. As Alice, use **Members** to change another member's role. A viewer can inspect records but write controls are disabled. Server authorization still checks every request independently.
4. Use **My notifications** to run the local notification worker as an administrator and read notifications addressed to the current subject. The control explicitly describes its global worker scope.
5. To exercise a conflict, open the same item in two tabs. Save in one, then save the older draft in the other. The second tab retains its inputs, displays the conflict and offers **Load latest version**. Loading updates the revision, preserves the assignment draft and adjusts valid status choices. Review, then submit explicitly. No write is retried automatically.

Blank/invalid inputs are validated with the same browser-safe Zod contracts used by the TS application. Server validation remains authoritative for both backends. Submitting disables the form and displays `Saving…`; denied or failed operations keep the draft. A permission denial refreshes the session and disables all write controls if access was revoked. Refreshing the list also refreshes permissions, so newly granted access can be used without changing tokens.

Pages and individual editors own AbortControllers. Switching pages, records, accounts or backends cancels obsolete requests and prevents late responses from repainting the new view. Cancellation is not a rollback guarantee: an already committed write may still exist, and the UI never automatically repeats it.

## Extension and SDK integration

`ConsolePages<View>` registers the four compiled factories. Navigation comes from the authorized manifest. Each view returns its native element, a ready Promise and a dispose method; the host owns those lifetimes. All business calls use `ConsoleClient`, and every known result is validated with `ConsoleClient.callParsed` before use. The browser imports only `@lenso/example-team-desk/contracts`, never the Node database implementation.

Connecting this consumer exposed and fixed a real browser defect: the default fetch transport lost its global receiver when invoked as a client method. The SDK now binds that default correctly; a failed-first unit regression and actual Chromium flows cover it. Injected transports remain supported.

The backend SDK now supports authorized operations not attached to a page control: the TS extension constructor's third argument and Rust `ConsoleExtension::register`. Team Desk uses these for `members.session` and `notifications.drain`. They retain Executor validation, authorization and records, and must be present in the catalog when a manifest is built. Unknown ids remain undispatchable.

## Design and interaction rules

The design read is an administrative workbench with low visual energy, stable table/editor rhythm and minimal motion: **ENERGY 1 / RHYTHM 2 / MOTION 1**. The primary attention tier is the record list and current editor; navigation/session are persistent context; delivery mechanics and errors appear where they matter. Native system typography keeps table text and form controls consistent across the supported local browser environment. Green identifies primary work and selected navigation, structural borders separate information, and amber identifies a recoverable conflict. No illustrative assets, fabricated metrics or decorative card grid are used.

The 390px layout moves the editor below the table and keeps navigation, connection fields and actions usable. Fields have labels, associated errors and visible keyboard focus; Enter submits a form. Table rows and editors use the same names and verbs as the application contract. Short hover transitions respect reduced motion. Only the light theme is implemented.

## Browser verification

After building both backends and the host:

```sh
npx playwright install chromium
npm run test:browser
# Optional second engine:
npx playwright install webkit
CONSOLE_BROWSER=webkit npm run test:browser
```

The Node test suite starts real backend processes and serves the production browser bundle. It tests both backends with the same page implementation: validation, keyboard submission, pending writes, role changes, read-only controls, private notifications, version conflicts with explicit retry, archive behavior and mobile overflow. Additional cases verify live revocation, backend isolation, stale page responses and switching records during a committed but delayed write response. The suite captures screenshots under `/tmp`, outside the repository. An explicit `CONSOLE_BROWSER_EXECUTABLE` can select an existing compatible browser for local diagnostics; default Chromium runs use Playwright's bundled Chromium channel.

The references used for the host and browser harness are the official [Vite proxy options](https://vite.dev/config/server-options#server-proxy) and [Playwright library documentation](https://playwright.dev/docs/library). Executed environment/results are tracked in the repository [verification record](../../../docs/verification.md).

This host proves the Team Desk browser workflow. It is not the complete Official Console product, a production identity integration or a deployment platform.

The browser-consumer review removed the host's parsing shim and separate session parsing in favor of the SDK's parsed-result path. This keeps malformed response errors separate from form input errors, including after an already committed write. Concurrent session refreshes use newest-request ordering, so a delayed old role response cannot replace a newer revocation. The new cases run in both verified browser engines.
