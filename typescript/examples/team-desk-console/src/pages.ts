import { z } from "zod";
import type { ConsoleViewContext } from "@lenso/console";
import {
  projectSchema, itemSchema, memberSchema, changeSchema, projectInput, itemInput, assignInput, transitionInput, setRoleInput,
} from "@lenso/example-team-desk/contracts";
import type { Project, Item } from "@lenso/example-team-desk/contracts";
import { bindForm, canWrite, conflictPanel, editorHeading, escape, formShell, inputField, isAdmin, node, selectField, table, text, value, workspace } from "./ui.js";
import type { Host, View } from "./ui.js";

const roles = ["admin", "editor", "viewer"].map((value) => ({ value, label: value }));
const statusLabel = { open: "Open", doing: "In progress", done: "Done" };
function addCreate(element: HTMLElement, label: string, allowed: () => boolean, run: () => void) {
  const button = document.createElement("button");
  button.type = "button"; button.className = "primary"; button.textContent = label; button.disabled = !allowed();
  button.addEventListener("click", run); node(element, ".page-actions").append(button);
  element.addEventListener("sessionchange", () => { button.disabled = !allowed(); });
  return button;
}

export function projectsPage(context: ConsoleViewContext, host: Host): View {
  const view = workspace(context, host, "Plan work, inspect revisions, and close projects when the work is complete.");
  const { element, signal } = view, editor = node(element, ".editor");
  let projects: Project[] = [];
  const load = async () => {
    try {
      const rows = await context.client.callParsed(context.page.list, z.array(projectSchema), null, signal);
      if (signal.aborted) return;
      projects = rows;
      node(element, "[data-table]").innerHTML = table(["Project", "State", "Version"], rows.map((row) => `<tr><td><button class="record-link" data-project="${escape(row.id)}">${escape(row.name)}</button><small>${escape(row.id)}</small></td><td>${row.archived ? "Archived" : "Active"}</td><td class="numeric">${row.version}</td></tr>`), "No projects yet");
      text(element, "[data-load]", `${rows.length} project${rows.length === 1 ? "" : "s"}`);
      view.showPermission();
    } catch (error) { view.report(error); }
  };
  function create() {
    const signal = view.edit();
    editor.innerHTML = editorHeading("New project", "Give the project a stable ID and a clear name.") + formShell(inputField("Project ID", "id", "", 64) + inputField("Project name", "name", "", 120), "Create project");
    const form = node<HTMLFormElement>(editor, "form");
    bindForm({ form, signal, host, allowed: () => canWrite(host),
      parse: () => projectInput.parse({ id: value(form, "id"), name: value(form, "name") }),
      async submit(input) {
        const project = await context.client.callParsed("projects.create", projectSchema, input, signal);
        if (signal.aborted) return;
        form.reset(); text(form, ".form-status", `Created ${project.name}.`); await load();
      },
    });
    node<HTMLInputElement>(form, "input").focus();
  }
  function detail(project: Project) {
    const signal = view.edit();
    let current = project;
    editor.innerHTML = editorHeading(project.name, `Project ${project.id}`) + `<dl><dt>State</dt><dd data-state></dd><dt>Version</dt><dd data-version></dd></dl><p>Archiving stops changes to this project's work items.</p>` + formShell("", "Archive project");
    const form = node<HTMLFormElement>(editor, "form");
    const update = () => { text(editor, "[data-state]", current.archived ? "Archived" : "Active"); text(editor, "[data-version]", String(current.version)); };
    const permissions = bindForm({ form, signal, host, allowed: () => isAdmin(host) && !current.archived,
      parse: () => ({ id: current.id, version: current.version }),
      async submit(input) {
        current = await context.client.callParsed("projects.archive", projectSchema, input, signal);
        if (signal.aborted) return;
        update(); text(form, ".form-status", "Project archived."); await load();
      },
      onConflict: () => conflictPanel(editor, signal, async () => {
        current = await context.client.callParsed(context.page.detail!, projectSchema, { id: current.id }, signal);
        if (!signal.aborted) { update(); permissions(); await load(); }
      }),
    });
    update();
  }
  view.refresh(load);
  node(element, "[data-table]").addEventListener("click", (event) => {
    const id = event.target instanceof Element ? event.target.closest<HTMLElement>("[data-project]")?.dataset.project : undefined;
    const project = projects.find((row) => row.id === id); if (project) detail(project);
  });
  addCreate(element, "New project", () => canWrite(host), create);
  return { ...view, ready: load() };
}

export function itemsPage(context: ConsoleViewContext, host: Host): View {
  const view = workspace(context, host, "Assign work and move it forward. Every change checks the version you reviewed.");
  const { element, signal } = view, editor = node(element, ".editor");
  let items: Item[] = [], projects: Project[] = [], members: z.infer<typeof memberSchema>[] = [];
  const load = async () => {
    try {
      const [rows, projectRows, memberRows] = await Promise.all([
        context.client.callParsed(context.page.list, z.array(itemSchema), null, signal),
        context.client.callParsed("projects.list", z.array(projectSchema), null, signal),
        context.client.callParsed("members.list", z.array(memberSchema), null, signal),
      ]);
      if (signal.aborted) return;
      items = rows; projects = projectRows; members = memberRows;
      createButton.disabled = !canCreate();
      node(element, "[data-table]").innerHTML = table(["Work item", "Project", "Assignee", "Status", "Version"], rows.map((row) => `<tr><td><button class="record-link" data-item="${escape(row.id)}">${escape(row.title)}</button><small>${escape(row.id)}</small></td><td>${escape(projects.find((project) => project.id === row.project)?.name ?? row.project)}</td><td>${escape(row.assignee)}</td><td><span class="state state-${row.status}">${statusLabel[row.status]}</span></td><td class="numeric">${row.version}</td></tr>`), "No work items yet");
      text(element, "[data-load]", `${rows.length} work item${rows.length === 1 ? "" : "s"}`); view.showPermission();
    } catch (error) { view.report(error); }
  };
  const assignees = () => members.filter((member) => member.role !== "viewer").map((member) => ({ value: member.subject, label: member.subject }));
  function create() {
    const signal = view.edit();
    editor.innerHTML = editorHeading("New work item", "Choose an active project and a team member who can edit work.") + formShell(
      inputField("Work item ID", "id", "", 64) + inputField("Title", "title") + selectField("Project", "project", projects.filter((project) => !project.archived).map((project) => ({ value: project.id, label: project.name }))) + selectField("Assignee", "assignee", assignees()), "Create work item");
    const form = node<HTMLFormElement>(editor, "form");
    bindForm({ form, signal, host, allowed: () => canWrite(host),
      parse: () => itemInput.parse({ id: value(form, "id"), title: value(form, "title"), project: value(form, "project"), assignee: value(form, "assignee") }),
      async submit(input) {
        const item = await context.client.callParsed("items.create", itemSchema, input, signal);
        if (signal.aborted) return;
        form.reset(); text(form, ".form-status", `Created ${item.title}.`); await load();
      },
    });
    node<HTMLInputElement>(form, "input").focus();
  }
  function detail(item: Item) {
    const signal = view.edit();
    let current = item;
    editor.innerHTML = editorHeading(item.title, `Work item ${item.id}`) + `<dl><dt>Status</dt><dd data-current-state></dd><dt>Version</dt><dd data-current-version></dd></dl><p class="archived-note"></p><section class="edit-section"><h3>Assignment</h3>${formShell(selectField("Assign to", "assignee", assignees(), item.assignee), "Save assignment")}</section><section class="edit-section"><h3>Progress</h3>${formShell(selectField("Next status", "status", [{ value: "doing", label: "In progress" }, { value: "done", label: "Done" }], item.status === "open" ? "doing" : "done"), "Change status")}</section>`;
    const [assignment, transition] = Array.from(editor.querySelectorAll("form"));
    if (!assignment || !transition) throw new Error("Missing item forms");
    const allowed = () => canWrite(host) && projects.some((project) => project.id === current.project && !project.archived);
    const update = () => {
      text(editor, "[data-current-state]", statusLabel[current.status]); text(editor, "[data-current-version]", String(current.version));
      text(editor, ".archived-note", projects.find((project) => project.id === current.project)?.archived ? "This project is archived. Work items are read-only." : "");
      const target = node<HTMLSelectElement>(transition, "select");
      for (const option of target.options) option.disabled = !((current.status === "open" && option.value === "doing") || (current.status === "doing" && option.value === "done"));
      if (target.selectedOptions[0]?.disabled && current.status !== "done") target.value = current.status === "open" ? "doing" : "done";
    };
    const refresh = () => conflictPanel(editor, signal, async () => {
      current = await context.client.callParsed(context.page.detail!, itemSchema, { id: current.id }, signal);
      if (signal.aborted) return;
      await load();
      if (!signal.aborted) { update(); assignPermissions(); transitionPermissions(); }
    });
    const assignPermissions = bindForm({ form: assignment, signal, host, allowed,
      parse: () => assignInput.parse({ id: current.id, version: current.version, assignee: value(assignment, "assignee") }),
      async submit(input) {
        current = await context.client.callParsed("items.assign", itemSchema, input, signal);
        if (signal.aborted) return;
        update(); text(assignment, ".form-status", "Assignment saved."); await load(); transitionPermissions();
      }, onConflict: refresh,
    });
    const transitionPermissions = bindForm({ form: transition, signal, host, allowed: () => allowed() && current.status !== "done",
      parse: () => transitionInput.parse({ id: current.id, version: current.version, status: value(transition, "status") }),
      async submit(input) {
        current = await context.client.callParsed("items.transition", itemSchema, input, signal);
        if (signal.aborted) return;
        update(); text(transition, ".form-status", "Status saved.");
        node<HTMLSelectElement>(transition, "select").value = current.status === "open" ? "doing" : "done";
        await load(); assignPermissions();
      }, onConflict: refresh,
    });
    update();
  }
  view.refresh(load);
  node(element, "[data-table]").addEventListener("click", (event) => {
    const id = event.target instanceof Element ? event.target.closest<HTMLElement>("[data-item]")?.dataset.item : undefined;
    const item = items.find((row) => row.id === id); if (item) detail(item);
  });
  const canCreate = () => canWrite(host) && projects.some((project) => !project.archived) && members.some((member) => member.role !== "viewer");
  const createButton = addCreate(element, "New work item", canCreate, create);
  return { ...view, ready: load() };
}

export function membersPage(context: ConsoleViewContext, host: Host): View {
  const view = workspace(context, host, "Roles apply immediately to existing sessions. You cannot change your own role.");
  const { element, signal } = view, editor = node(element, ".editor");
  let members: z.infer<typeof memberSchema>[] = [];
  const load = async () => {
    try {
      const rows = await context.client.callParsed(context.page.list, z.array(memberSchema), null, signal);
      if (signal.aborted) return;
      members = rows;
      node(element, "[data-table]").innerHTML = table(["Member", "Role"], rows.map((member) => `<tr><td><button class="record-link" data-member="${escape(member.subject)}">${escape(member.subject)}</button></td><td>${escape(member.role)}</td></tr>`), "No members");
      text(element, "[data-load]", `${rows.length} team members`); view.showPermission();
    } catch (error) { view.report(error); }
  };
  node(element, "[data-table]").addEventListener("click", (event) => {
    const subject = event.target instanceof Element ? event.target.closest<HTMLElement>("[data-member]")?.dataset.member : undefined;
    const member = members.find((row) => row.subject === subject); if (!member) return;
    const signal = view.edit();
    editor.innerHTML = editorHeading(member.subject, member.subject === host.session().subject ? "Your role cannot be changed in this session." : "Choose the access this member needs.") + formShell(selectField("Role", "role", roles, member.role), "Save role");
    const form = node<HTMLFormElement>(editor, "form");
    bindForm({ form, signal, host, allowed: () => isAdmin(host) && member.subject !== host.session().subject,
      parse: () => setRoleInput.parse({ subject: member.subject, role: value(form, "role") }),
      async submit(input) { await context.client.callParsed("members.setRole", memberSchema, input, signal); if (!signal.aborted) { text(form, ".form-status", "Role saved. Existing sessions use this permission immediately."); await load(); } },
    });
  });
  view.refresh(load);
  return { ...view, ready: load() };
}

export function notificationsPage(context: ConsoleViewContext, host: Host): View {
  const view = workspace(context, host, "Your private inbox of persisted work-item changes.");
  const { element, signal } = view;
  element.classList.add("inbox");
  const editor = node(element, ".editor");
  editor.innerHTML = editorHeading("Notification delivery", "Changes are queued in the same transaction as the work item.") + `<p>The worker processes pending notifications across all teams. Your inbox only shows messages addressed to you.</p>` + formShell("", "Run notification worker");
  const load = async () => {
    try {
      const rows = await context.client.callParsed(context.page.list, z.array(changeSchema), null, signal);
      if (signal.aborted) return;
      node(element, "[data-table]").innerHTML = table(["Work item", "Change", "Actor", "Version"], rows.map((row) => `<tr><td>${escape(row.item)}</td><td>${escape(row.kind)}</td><td>${escape(row.actor)}</td><td>${row.version}</td></tr>`), "Your inbox is clear");
      text(element, "[data-load]", `${rows.length} notification${rows.length === 1 ? "" : "s"}`);
    } catch (error) { view.report(error); }
  };
  const form = node<HTMLFormElement>(editor, "form");
  bindForm({ form, signal, host, allowed: () => isAdmin(host), parse: () => null,
    async submit(input) { const result = await context.client.callParsed("notifications.drain", z.object({ completed: z.number().int() }), input, signal); if (!signal.aborted) { text(form, ".form-status", `Delivered ${result.completed} queued notification${result.completed === 1 ? "" : "s"}.`); await load(); } },
  });
  view.refresh(load);
  return { ...view, ready: load() };
}
