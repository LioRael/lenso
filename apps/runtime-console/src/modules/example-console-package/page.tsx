const packageRows = [
  ["Package", "@lenso/example-console"],
  ["Export", "exampleConsoleModule"],
  ["Route", "/runtime/example-console"],
  ["Source", "installed"],
] as const;

const lifecycleRows = [
  ["Resolve", "registry key maps package export to a console module"],
  ["Select", "metadata contributes the package reference to navigation"],
  ["Mount", "router creates the page from the module contribution"],
] as const;

export function ExampleConsolePage() {
  return (
    <main className="flex h-full flex-col gap-6 overflow-auto bg-background p-6">
      <header className="space-y-1">
        <p className="font-medium text-muted-foreground text-xs uppercase tracking-normal">
          Console package
        </p>
        <h1 className="font-semibold text-2xl text-foreground">
          Installed package example
        </h1>
      </header>

      <section className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(320px,420px)]">
        <div className="rounded-md border border-border bg-card">
          <div className="border-border border-b px-4 py-3">
            <h2 className="font-medium text-foreground text-sm">
              Package reference
            </h2>
          </div>
          <dl className="divide-y divide-border">
            {packageRows.map(([label, value]) => (
              <div
                className="grid grid-cols-[120px_minmax(0,1fr)] gap-3 px-4 py-3 text-sm"
                key={label}
              >
                <dt className="text-muted-foreground">{label}</dt>
                <dd className="font-mono text-foreground text-xs">{value}</dd>
              </div>
            ))}
          </dl>
        </div>

        <div className="rounded-md border border-border bg-card">
          <div className="border-border border-b px-4 py-3">
            <h2 className="font-medium text-foreground text-sm">
              Host boundary
            </h2>
          </div>
          <div className="space-y-3 px-4 py-3 text-muted-foreground text-sm">
            <p>
              This page is loaded through the console package registry instead
              of a page-level import.
            </p>
            <p>
              A real module can replace this package with its own compiled
              frontend export when the installer boundary is wired to package
              installation.
            </p>
          </div>
        </div>
      </section>

      <section className="rounded-md border border-border bg-card">
        <div className="border-border border-b px-4 py-3">
          <h2 className="font-medium text-foreground text-sm">
            Runtime console mount path
          </h2>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full min-w-[620px] text-left text-sm">
            <thead className="border-border border-b text-muted-foreground">
              <tr>
                <th className="px-4 py-2 font-medium">Step</th>
                <th className="px-4 py-2 font-medium">Behavior</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {lifecycleRows.map(([step, behavior]) => (
                <tr key={step}>
                  <td className="px-4 py-3 font-medium text-foreground">
                    {step}
                  </td>
                  <td className="px-4 py-3 text-muted-foreground">
                    {behavior}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </main>
  );
}
