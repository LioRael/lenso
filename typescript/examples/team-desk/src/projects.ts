import { z } from "zod";
import { ActionError, Module, defineAction, emptyInput } from "@lenso/app";
import { Store, sameVersion } from "./store.js";
import { id, projectSchema, projectInput, version } from "./contracts.js";

export function projectsModule() {
  const action = defineAction<Store>();
  const list = action({
    id: "projects.list", input: emptyInput, output: z.array(projectSchema),
    authorize(context) { context.resources.access(context); },
    run(context) { return z.array(projectSchema).parse(context.resources.db.prepare("SELECT id,name,archived,version FROM desk_projects WHERE team=? ORDER BY id").all(context.resources.team(context))); },
  });
  const detail = action({
    id: "projects.detail", input: z.object({ id }), output: projectSchema,
    authorize(context) { context.resources.access(context); },
    run(context, input) { return context.resources.project(context.resources.team(context), input.id); },
  });
  const create = action({
    id: "projects.create", input: projectInput, output: projectSchema,
    authorize(context) { context.resources.access(context, "write"); },
    run(context, input) {
      const store = context.resources, team = store.team(context);
      const changed = store.db.prepare("INSERT INTO desk_projects(team,id,name) VALUES(?,?,?) ON CONFLICT DO NOTHING").run(team, input.id, input.name);
      if (!changed.changes) throw new ActionError("conflict", "Project already exists");
      return store.project(team, input.id);
    },
  });
  const archive = action({
    id: "projects.archive", input: z.object({ id, version }), output: projectSchema,
    authorize(context) { context.resources.access(context, "admin"); },
    run(context, input) {
      const store = context.resources, team = store.team(context);
      return store.transaction(() => {
        const project = store.project(team, input.id);
        sameVersion(project.version, input.version);
        if (project.archived) throw new ActionError("conflict", "Project is archived");
        store.db.prepare("UPDATE desk_projects SET archived=1,version=version+1 WHERE team=? AND id=?").run(team, input.id);
        return store.project(team, input.id);
      });
    },
  });
  return {
    module: new Module("projects", { actions: { list, detail, create, archive }, resources: ["sqlite"] }),
    pages: [{ id: "projects", title: "Projects", list, detail, actions: [create, archive] }],
    actions: { list, detail, create, archive },
  };
}
