import { ConsoleClient, ConsolePages } from "@lenso/console";
import type { ConsoleManifest } from "@lenso/app";
import { sessionSchema } from "@lenso/example-team-desk/contracts";
import type { Session } from "@lenso/example-team-desk/contracts";
import { itemsPage, membersPage, notificationsPage, projectsPage } from "./pages.js";
import { message, node, text } from "./ui.js";
import type { Host, View } from "./ui.js";
import "./style.css";

const root = node<HTMLElement>(document, "#app");
root.innerHTML = `<div class="console-shell"><aside class="sidebar"><div class="wordmark">Team Desk<span>Console</span></div><p class="nav-label">Workspace</p><nav aria-label="Workspace pages"></nav><div class="session-info"><p data-subject>Not connected</p><p data-access>Connect to a local application.</p></div></aside><div class="main-shell"><header class="connection-bar"><form id="connection" aria-label="Connection"><label>Backend<select name="backend" aria-label="Backend"><option value="typescript">TypeScript</option><option value="rust">Rust</option></select></label><label>Account<select name="account" aria-label="Account"><option value="alice">Alice (alpha)</option><option value="bob">Bob (alpha)</option><option value="reader">Reader (alpha)</option><option value="eve">Eve (beta)</option><option value="custom">Custom token</option></select></label><label class="token-field">Bearer token<input type="password" name="token" aria-label="Bearer token" autocomplete="off" value="alice-local-secret-token"></label><button type="submit">Connect</button></form><p role="status" id="connection-status">Use a local demo account or enter a token. Tokens stay in this tab's memory.</p></header><main id="content"><section class="welcome"><h1>One workspace.<br>Two native backends.</h1><p>Connect to inspect your team's projects, manage work, and review changes. The same pages use the selected application's permissions.</p><p class="welcome-note">Demo credentials are public local fixtures. No external service is contacted.</p></section></main><footer>Team Desk application · Independent Console host</footer></div></div>`;

const connection = node<HTMLFormElement>(root, "#connection");
const backend = node<HTMLSelectElement>(connection, '[name="backend"]');
const account = node<HTMLSelectElement>(connection, '[name="account"]');
const tokenInput = node<HTMLInputElement>(connection, '[name="token"]');
const connectButton = node<HTMLButtonElement>(connection, "button");
const connectionControls = Array.from(connection.querySelectorAll<HTMLInputElement | HTMLSelectElement | HTMLButtonElement>("input,select,button"));
const nav = node(root, "nav");
const content = node(root, "#content");
const demoTokens: Record<string, string> = { alice: "alice-local-secret-token", bob: "bob-local-secret-token", reader: "reader-local-secret-token", eve: "eve-local-secret-token" };
account.addEventListener("change", () => { tokenInput.value = demoTokens[account.value] ?? ""; if (account.value === "custom") tokenInput.focus(); });
let active: View | undefined;
let connectionScope: AbortController | undefined;
let pages: ConsolePages<View> | undefined;
let manifest: ConsoleManifest | undefined;
let currentPage = "projects";
let session: Session | undefined;

function identity(value: Session) {
  text(root, "[data-subject]", value.subject);
  text(root, "[data-access]", `${value.team} · ${value.role === "viewer" ? "Read-only" : value.role}`);
}
function renderPage(id: string) {
  const descriptor = manifest?.pages.find((page) => page.id === id);
  if (!descriptor || !pages) return;
  active?.dispose();
  currentPage = id;
  for (const button of nav.querySelectorAll("button")) {
    if (button.dataset.page === id) button.setAttribute("aria-current", "page"); else button.removeAttribute("aria-current");
  }
  try {
    const view = pages.render(descriptor);
    active = view;
    content.replaceChildren(view.element);
    void view.ready.catch((error: unknown) => { if (active === view) text(view.element, "[data-load]", message(error)); });
  } catch (error) {
    content.innerHTML = '<section class="welcome"><h1>Page unavailable</h1><p></p></section>';
    text(content, "p", message(error));
  }
}
connection.addEventListener("submit", async (event) => {
  event.preventDefault();
  connectionScope?.abort(); active?.dispose(); active = undefined;
  const scope = new AbortController(); connectionScope = scope;
  pages = undefined; manifest = undefined; session = undefined; nav.replaceChildren();
  const runtime = backend.value;
  const token = tokenInput.value;
  for (const control of connectionControls) control.disabled = true;
  connectButton.textContent = "Connecting…";
  text(root, "#connection-status", "Connecting to the application…");
  content.innerHTML = '<section class="welcome"><h1>Connecting</h1><p role="status">Checking the session and installed page extensions…</p></section>';
  const client = new ConsoleClient(new URL(`/backends/${runtime}/console/`, location.origin).href, () => token);
  try {
    const [description, subject] = await Promise.all([client.manifest(scope.signal), client.callParsed("members.session", sessionSchema, null, scope.signal)]);
    if (scope.signal.aborted) return;
    session = subject; manifest = description;
    let refreshSequence = 0;
    const host: Host = {
      session() { if (!session) throw new Error("Session is not connected"); return session; },
      async refreshSession() {
        const sequence = ++refreshSequence;
        const latest = await client.callParsed("members.session", sessionSchema, null, scope.signal);
        if (!scope.signal.aborted && sequence === refreshSequence) { session = latest; identity(latest); active?.element.dispatchEvent(new Event("sessionchange")); }
      },
    };
    pages = new ConsolePages<View>(client);
    pages.register({ id: "team-desk", pages: {
      projects: (context) => projectsPage(context, host),
      items: (context) => itemsPage(context, host),
      members: (context) => membersPage(context, host),
      notifications: (context) => notificationsPage(context, host),
    } });
    for (const descriptor of description.pages) {
      const button = document.createElement("button");
      button.type = "button"; button.textContent = descriptor.title; button.dataset.page = descriptor.id;
      button.addEventListener("click", () => renderPage(descriptor.id)); nav.append(button);
    }
    identity(session);
    text(root, "#connection-status", `Connected to ${runtime === "rust" ? "Rust" : "TypeScript"}. Server permissions apply to every change.`);
    renderPage(description.pages.some((page) => page.id === currentPage) ? currentPage : description.pages[0]?.id ?? "");
  } catch (error) {
    if (scope.signal.aborted) return;
    text(root, "#connection-status", message(error));
    text(root, "[data-subject]", "Not connected"); text(root, "[data-access]", "Check your token and backend.");
    content.innerHTML = '<section class="welcome"><h1>Connection unavailable</h1><p>Review the backend and token above, then connect again.</p></section>';
  } finally {
    if (!scope.signal.aborted) { for (const control of connectionControls) control.disabled = false; connectButton.textContent = "Connect"; }
  }
});
