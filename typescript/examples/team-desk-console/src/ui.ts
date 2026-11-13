import { ActionError } from "@lenso/app";
import type { ConsoleViewContext } from "@lenso/console";
import { ZodError } from "zod";
import type { Session } from "@lenso/example-team-desk/contracts";

export interface View { element: HTMLElement; dispose(): void; ready: Promise<void> }
export interface Host {
  session(): Session;
  refreshSession(): Promise<void>;
}
export function node<T extends HTMLElement>(root: ParentNode, selector: string): T {
  const value = root.querySelector<T>(selector);
  if (!value) throw new Error(`Missing element: ${selector}`);
  return value;
}
export function escape(value: string | number): string {
  const entities: Record<string, string> = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
  return String(value).replace(/[&<>"']/g, (char) => entities[char] ?? char);
}
export function text(root: ParentNode, selector: string, value: string) { node(root, selector).textContent = value; }
export function message(error: unknown): string {
  if (error instanceof ActionError) {
    if (error.code === "forbidden") return "Your permission changed or this action is not allowed. Your draft has been kept.";
    if (error.code === "unauthenticated") return "This session is no longer valid. Connect again with a valid token.";
    return error.message;
  }
  if (error instanceof ZodError) return "The server returned data this page cannot read.";
  return "Could not reach the application. Check the connection and refresh before retrying a write.";
}
export function canWrite(host: Host) { return host.session().role !== "viewer"; }
export function isAdmin(host: Host) { return host.session().role === "admin"; }
export function workspace(context: ConsoleViewContext, host: Host, description: string) {
  const element = document.createElement("section");
  element.className = "workspace";
  element.innerHTML = `<header class="page-header"><div><h1>${escape(context.page.title)}</h1><p>${escape(description)}</p></div><div class="page-actions"><button type="button" data-refresh>Refresh list</button></div></header>
    <p class="permission" data-permission></p><p class="load-status" role="status" data-load>Loading…</p>
    <div class="work-area"><section class="records" aria-label="Records"><div class="table-wrap" data-table></div></section>
    <aside class="editor" aria-label="Record editor"><div class="editor-placeholder"><h2>Record details</h2><p>Select a row to inspect its current version and available actions.</p></div></aside></div>`;
  const controller = new AbortController();
  let editorScope = new AbortController();
  controller.signal.addEventListener("abort", () => editorScope.abort(), { once: true });
  const edit = () => { editorScope.abort(); editorScope = new AbortController(); return editorScope.signal; };
  const showPermission = () => text(element, "[data-permission]", canWrite(host) ? `Team ${host.session().team}. Changes are checked by the application.` : "Read-only access. You can inspect records, but cannot change them.");
  showPermission();
  element.addEventListener("sessionchange", showPermission);
  const report = (error: unknown) => { if (!controller.signal.aborted) { text(element, "[data-load]", message(error)); node(element, "[data-load]").setAttribute("role", "alert"); } };
  const refresh = (load: () => Promise<void>) => {
    const button = node<HTMLButtonElement>(element, "[data-refresh]");
    button.addEventListener("click", async () => {
      button.disabled = true;
      try { await host.refreshSession(); if (!controller.signal.aborted) await load(); }
      catch (error) { report(error); }
      finally { if (!controller.signal.aborted) button.disabled = false; }
    }, { signal: controller.signal });
  };
  return { element, signal: controller.signal, edit, refresh, dispose: () => controller.abort(), showPermission, report };
}
export function formField(label: string, name: string, control: string): string {
  return `<div class="field"><label for="${escape(name)}">${escape(label)}</label>${control}<p class="field-error" data-error="${escape(name)}" id="error-${escape(name)}"></p></div>`;
}
export function inputField(label: string, name: string, value = "", maxLength = 200): string {
  return formField(label, name, `<input id="${escape(name)}" name="${escape(name)}" value="${escape(value)}" maxlength="${maxLength}" aria-describedby="error-${escape(name)}" autocomplete="off">`);
}
export function selectField(label: string, name: string, options: readonly { value: string; label: string }[], selected = ""): string {
  return formField(label, name, `<select id="${escape(name)}" name="${escape(name)}" aria-describedby="error-${escape(name)}">${options.map((option) => `<option value="${escape(option.value)}" ${option.value === selected ? "selected" : ""}>${escape(option.label)}</option>`).join("")}</select>`);
}
export function value(form: HTMLFormElement, name: string): string { return String(new FormData(form).get(name) ?? ""); }
export function formShell(fields: string, submit: string): string {
  return `<form novalidate><fieldset>${fields}<button class="primary" type="submit">${escape(submit)}</button></fieldset><p class="form-status" role="status"></p></form>`;
}
export function bindForm<T>(options: {
  form: HTMLFormElement; signal: AbortSignal; host: Host; allowed(): boolean; parse(): T;
  submit(input: T): Promise<void>; onConflict?(): void;
}) {
  const { form, signal, host } = options;
  const fieldset = node<HTMLFieldSetElement>(form, "fieldset");
  const button = node<HTMLButtonElement>(form, '[type="submit"]');
  const status = node(form, ".form-status");
  const label = button.textContent;
  let busy = false;
  const permissions = () => { fieldset.disabled = busy || !options.allowed(); };
  permissions();
  form.closest(".workspace")?.addEventListener("sessionchange", permissions, { signal });
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (busy || !options.allowed()) return;
    for (const error of form.querySelectorAll("[data-error]")) error.textContent = "";
    for (const input of form.querySelectorAll("[aria-invalid]")) input.removeAttribute("aria-invalid");
    status.textContent = ""; status.setAttribute("role", "status");
    try {
      const input = options.parse();
      busy = true; permissions(); button.textContent = "Saving…"; form.setAttribute("aria-busy", "true");
      await options.submit(input);
    } catch (error) {
      if (signal.aborted) return;
      status.setAttribute("role", "alert");
      if (error instanceof ZodError) {
        status.textContent = "Check the highlighted fields.";
        for (const issue of error.issues) {
          const name = String(issue.path[0]);
          const field = form.elements.namedItem(name);
          if (field instanceof HTMLElement) field.setAttribute("aria-invalid", "true");
          const feedback = form.querySelector(`[data-error="${CSS.escape(name)}"]`);
          if (feedback && !feedback.textContent) feedback.textContent = issue.message;
        }
        form.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
      } else {
        status.textContent = message(error);
        if (error instanceof ActionError && error.code === "conflict") options.onConflict?.();
        if (error instanceof ActionError && error.code === "forbidden") {
          try { await host.refreshSession(); } catch { status.textContent = "Your session could not be refreshed. Connect again. Your draft has been kept."; }
        }
      }
    } finally {
      busy = false;
      if (!signal.aborted) { button.textContent = label; form.removeAttribute("aria-busy"); permissions(); }
    }
  }, { signal });
  return permissions;
}
export function table(headers: string[], rows: string[], empty: string): string {
  if (!rows.length) return `<div class="empty"><h2>${escape(empty)}</h2><p>New records will appear here after they are saved.</p></div>`;
  return `<table><thead><tr>${headers.map((header) => `<th scope="col">${escape(header)}</th>`).join("")}</tr></thead><tbody>${rows.join("")}</tbody></table>`;
}
export function editorHeading(title: string, detail: string): string {
  return `<div class="editor-heading"><h2>${escape(title)}</h2><p>${escape(detail)}</p></div><div data-conflict></div>`;
}
export function conflictPanel(editor: HTMLElement, signal: AbortSignal, reload: () => Promise<void>) {
  const panel = node(editor, "[data-conflict]");
  panel.innerHTML = `<div class="conflict" role="alert"><h3>This record changed</h3><p>Your draft has been kept. Load the latest version, review the changes, then submit again.</p><button type="button">Load latest version</button></div>`;
  const button = node<HTMLButtonElement>(panel, "button");
  button.addEventListener("click", async () => {
    button.disabled = true;
    try {
      await reload();
      if (!signal.aborted) {
        panel.replaceChildren();
        for (const status of editor.querySelectorAll(".form-status")) { status.textContent = "Latest version loaded. Your draft is preserved. Review and submit again."; status.setAttribute("role", "status"); }
      }
    } catch (error) { if (!signal.aborted) { panel.querySelector("p")!.textContent = message(error); button.disabled = false; } }
  });
}
