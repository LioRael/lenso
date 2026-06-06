import { runtimeConsoleHostApi } from "@lenso/runtime-console-api";

import { identityUserRows, identityUsersSummary } from "./model";

const surfaceRows = [
  ["Module", "identity"],
  ["Package", "@lenso/identity-console"],
  ["Export", "identityConsoleModule"],
  ["Route", "/data/identity"],
  ["Capability", "identity.users.read"],
] as const;

const userFields = [
  ["id", "String", "required"],
  ["email", "String", "required"],
  ["display_name", "String", "nullable"],
  ["created_at", "Timestamp", "required"],
  ["updated_at", "Timestamp", "required"],
] as const;

const workflowRows = [
  ["Schema", "Identity exposes Users through schema-admin"],
  ["Runtime", "identity.cleanup_expired_sessions.v1 is declared"],
  ["Stories", "registration and current-user routes carry story labels"],
] as const;

export function IdentityConsolePage() {
  const usersQuery = runtimeConsoleHostApi.adminData.useRecords({
    entityName: "users",
    moduleName: "identity",
  });
  const userRows = identityUserRows(usersQuery.data?.data ?? []);
  const summary = identityUsersSummary(usersQuery.data?.data ?? []);

  return (
    <main className="flex h-full flex-col gap-4 overflow-auto bg-background p-4">
      <header className="flex flex-wrap items-start gap-3 border-border border-b pb-3">
        <div className="min-w-0">
          <p className="font-medium text-muted-foreground text-xs uppercase tracking-normal">
            Module console package
          </p>
          <h1 className="font-semibold text-2xl text-foreground">Identity</h1>
        </div>
        <div className="ml-auto flex flex-wrap gap-2 text-xs">
          <span className="border border-border px-2 py-1 text-muted-foreground">
            linked module
          </span>
          <span className="border border-border px-2 py-1 text-muted-foreground">
            schema-admin
          </span>
        </div>
      </header>

      <section className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
        <div className="border border-border bg-card">
          <div className="border-border border-b px-3 py-2">
            <h2 className="font-medium text-foreground text-sm">
              User surface
            </h2>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full min-w-[560px] text-left text-sm">
              <thead className="border-border border-b text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 font-medium">Field</th>
                  <th className="px-3 py-2 font-medium">Type</th>
                  <th className="px-3 py-2 font-medium">Constraint</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {userFields.map(([field, type, constraint]) => (
                  <tr key={field}>
                    <td className="px-3 py-2 font-mono text-foreground text-xs">
                      {field}
                    </td>
                    <td className="px-3 py-2 text-muted-foreground">{type}</td>
                    <td className="px-3 py-2 text-muted-foreground">
                      {constraint}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        <div className="border border-border bg-card">
          <div className="border-border border-b px-3 py-2">
            <h2 className="font-medium text-foreground text-sm">
              Package contract
            </h2>
          </div>
          <dl className="divide-y divide-border">
            {surfaceRows.map(([label, value]) => (
              <div
                className="grid grid-cols-[96px_minmax(0,1fr)] gap-3 px-3 py-2 text-sm"
                key={label}
              >
                <dt className="text-muted-foreground">{label}</dt>
                <dd className="truncate font-mono text-foreground text-xs">
                  {value}
                </dd>
              </div>
            ))}
          </dl>
        </div>
      </section>

      <section className="border border-border bg-card">
        <div className="flex items-center gap-3 border-border border-b px-3 py-2">
          <h2 className="font-medium text-foreground text-sm">Users</h2>
          <span className="ml-auto border border-border px-2 py-0.5 text-muted-foreground text-xs">
            {summary.total} records
          </span>
          <span className="border border-border px-2 py-0.5 text-muted-foreground text-xs">
            latest {summary.latestCreatedAt}
          </span>
        </div>
        {usersQuery.isError ? (
          <p className="px-3 py-3 text-muted-foreground text-sm">
            Failed to load users: {String(usersQuery.error.message)}
          </p>
        ) : usersQuery.isPending ? (
          <p className="px-3 py-3 text-muted-foreground text-sm">
            Loading users...
          </p>
        ) : userRows.length === 0 ? (
          <p className="px-3 py-3 text-muted-foreground text-sm">
            No users found.
          </p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[720px] text-left text-sm">
              <thead className="border-border border-b text-muted-foreground">
                <tr>
                  <th className="px-3 py-2 font-medium">ID</th>
                  <th className="px-3 py-2 font-medium">Email</th>
                  <th className="px-3 py-2 font-medium">Display name</th>
                  <th className="px-3 py-2 font-medium">Created</th>
                  <th className="px-3 py-2 font-medium">Updated</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border">
                {userRows.map((user) => (
                  <tr key={user.id}>
                    <td className="px-3 py-2 font-mono text-foreground text-xs">
                      {user.id}
                    </td>
                    <td className="px-3 py-2 text-foreground">{user.email}</td>
                    <td className="px-3 py-2 text-muted-foreground">
                      {user.displayName}
                    </td>
                    <td className="px-3 py-2 font-mono text-muted-foreground text-xs">
                      {user.createdAt}
                    </td>
                    <td className="px-3 py-2 font-mono text-muted-foreground text-xs">
                      {user.updatedAt}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="border border-border bg-card">
        <div className="border-border border-b px-3 py-2">
          <h2 className="font-medium text-foreground text-sm">
            Module lifecycle links
          </h2>
        </div>
        <div className="grid divide-y divide-border md:grid-cols-3 md:divide-x md:divide-y-0">
          {workflowRows.map(([label, value]) => (
            <div className="min-w-0 px-3 py-3" key={label}>
              <div className="font-medium text-foreground text-sm">{label}</div>
              <div className="mt-1 text-muted-foreground text-xs">{value}</div>
            </div>
          ))}
        </div>
      </section>
    </main>
  );
}
