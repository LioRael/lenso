#!/usr/bin/env node
import { realpathSync } from "node:fs";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { Command } from "commander";

const readJson = async (filePath) =>
  JSON.parse(await readFile(filePath, "utf-8"));

const readJsonFromReference = async (reference) => {
  if (reference.startsWith("file:")) {
    return readJson(fileURLToPath(reference));
  }
  if (reference.startsWith("http://") || reference.startsWith("https://")) {
    const response = await fetch(reference);
    if (!response.ok) {
      throw new Error(
        `Failed to fetch module manifest: ${response.status} ${response.statusText}`
      );
    }
    return response.json();
  }
  return readJson(path.resolve(reference));
};

const queueWrite = (pendingWrites, filePath, content) => {
  pendingWrites.set(filePath, content);
};

const insertBeforeNeedle = (fileSource, entry, needle) => {
  if (fileSource.includes(entry.trim())) {
    return fileSource;
  }
  const index = fileSource.indexOf(needle);
  if (index === -1) {
    throw new Error(`Could not find insertion point: ${needle}`);
  }
  return `${fileSource.slice(0, index)}${entry}${fileSource.slice(index)}`;
};

const insertBeforeFirstNeedle = (fileSource, entry, needles) => {
  if (fileSource.includes(entry.trim())) {
    return fileSource;
  }
  for (const needle of needles) {
    if (fileSource.includes(needle)) {
      return insertBeforeNeedle(fileSource, entry, needle);
    }
  }
  return `${fileSource.trimEnd()}\n${entry}`;
};

const insertIntoLinkedModuleEntries = (fileSource, entry) => {
  if (fileSource.includes(entry.trim())) {
    return fileSource;
  }
  const entriesStart = fileSource.indexOf("const LINKED_MODULE_ENTRIES");
  if (entriesStart === -1) {
    throw new Error("Could not find LINKED_MODULE_ENTRIES in app-bootstrap");
  }
  const entriesEnd = fileSource.indexOf("];", entriesStart);
  if (entriesEnd === -1) {
    throw new Error("Could not find LINKED_MODULE_ENTRIES closing bracket");
  }
  return `${fileSource.slice(0, entriesEnd)}${entry}${fileSource.slice(
    entriesEnd
  )}`;
};

const appendToken = (value, token, beforeToken) => {
  const tokens = value.split(" ");
  if (tokens.includes(token)) {
    return value;
  }
  const beforeIndex = tokens.indexOf(beforeToken);
  if (beforeIndex === -1) {
    return [...tokens, token].join(" ");
  }
  return [
    ...tokens.slice(0, beforeIndex),
    token,
    ...tokens.slice(beforeIndex),
  ].join(" ");
};

const appendListItem = (items, item) =>
  items.includes(item) ? items : [...items, item];

const parseRemoteModuleEntries = (value) =>
  value
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => {
      const [name, ...baseUrlParts] = entry.split("=");
      return {
        baseUrl: baseUrlParts.join("=").trim(),
        name: name.trim(),
      };
    })
    .filter((entry) => entry.name && entry.baseUrl);

const formatRemoteModuleEntries = (entries) =>
  entries.map((entry) => `${entry.name}=${entry.baseUrl}`).join(",");

const consolePackageKey = ({ exportName, packageName }) =>
  `${packageName}#${exportName}`;

const sortObject = (object) =>
  Object.fromEntries(
    Object.entries(object).toSorted(([left], [right]) =>
      left.localeCompare(right)
    )
  );

const camelCase = (value) =>
  value.replaceAll(/-([a-z0-9])/gu, (_match, letter) => letter.toUpperCase());

const snakeCase = (value) => value.replaceAll("-", "_");

const pascalCase = (value) => {
  const camel = camelCase(value);
  return `${camel.charAt(0).toUpperCase()}${camel.slice(1)}`;
};

const exportStemFromPackageSlug = (packageSlugValue) => {
  const normalized = packageSlugValue.replace(/-console$/u, "");
  return `${camelCase(normalized)}Console`;
};

const rustConsoleArea = (areaName) => {
  const areaByName = {
    configuration: "Configuration",
    data: "Data",
    operations: "Operations",
    runtime: "Runtime",
  };
  const rustArea = areaByName[areaName];
  if (!rustArea) {
    throw new Error(`Unsupported console surface area: ${areaName}`);
  }
  return rustArea;
};

const titleCase = (value) =>
  value
    .split("-")
    .map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
    .join(" ");

const slugify = (value) =>
  value
    .trim()
    .toLowerCase()
    .replaceAll(/[^a-z0-9]+/gu, "-")
    .replaceAll(/^-|-$/gu, "");

const defaultIcon = (areaName) =>
  areaName === "runtime" ? "workflow" : "database";

const pathExists = async (filePath) => {
  try {
    await stat(filePath);
    return true;
  } catch (error) {
    if (error.code === "ENOENT") {
      return false;
    }
    throw error;
  }
};

const readTextIfExists = async (filePath) =>
  (await pathExists(filePath)) ? readFile(filePath, "utf-8") : "";

const upsertEnvValue = (source, key, value) => {
  const lines = source ? source.split("\n") : [];
  const keyPrefix = `${key}=`;
  const index = lines.findIndex((line) => line.startsWith(keyPrefix));
  if (index === -1) {
    const trimmed = source.trimEnd();
    return `${trimmed ? `${trimmed}\n` : ""}${key}=${value}\n`;
  }
  lines[index] = `${key}=${value}`;
  return `${lines.join("\n").replaceAll(/\n+$/gu, "")}\n`;
};

const findRepoRoot = async (startPath) => {
  let current = path.resolve(startPath);
  while (true) {
    if (
      (await pathExists(path.join(current, "Cargo.toml"))) &&
      (await pathExists(path.join(current, "crates/app-bootstrap")))
    ) {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) {
      return path.resolve(startPath);
    }
    current = parent;
  }
};

const relativePath = (runtimeConsoleRoot, filePath) =>
  path.relative(runtimeConsoleRoot, filePath);

const runtimeConsolePaths = (runtimeConsoleRoot) => ({
  manifestExportsPath: path.join(
    runtimeConsoleRoot,
    "src/console-package-manifest-exports.ts"
  ),
  moduleExportsPath: path.join(
    runtimeConsoleRoot,
    "src/console-package-module-exports.ts"
  ),
  oxlintConfigPath: path.join(runtimeConsoleRoot, "oxlint.config.ts"),
  packageJsonPath: path.join(runtimeConsoleRoot, "package.json"),
  tsconfigPath: path.join(runtimeConsoleRoot, "tsconfig.json"),
  viteConfigPath: path.join(runtimeConsoleRoot, "vite.config.ts"),
});

const repoPaths = (repoRoot) => ({
  appBootstrapCargoTomlPath: path.join(
    repoRoot,
    "crates/app-bootstrap/Cargo.toml"
  ),
  appBootstrapLibPath: path.join(repoRoot, "crates/app-bootstrap/src/lib.rs"),
  cargoTomlPath: path.join(repoRoot, "Cargo.toml"),
});

const updatePackageJson = async ({
  packageName,
  packageSlug,
  paths,
  pendingWrites,
}) => {
  const packageJson = await readJson(paths.packageJsonPath);
  packageJson.dependencies = sortObject({
    ...packageJson.dependencies,
    [packageName]: "workspace:*",
  });
  packageJson.scripts.test = appendToken(
    packageJson.scripts.test,
    `packages/${packageSlug}/src`,
    "packages/console-package-api/src"
  );
  queueWrite(
    pendingWrites,
    paths.packageJsonPath,
    `${JSON.stringify(packageJson, null, 2)}\n`
  );
};

const updateRuntimeConsoleDependency = ({
  dependencyVersion,
  packageJson,
  packageName,
}) => {
  packageJson.dependencies = sortObject({
    ...packageJson.dependencies,
    [packageName]: packageJson.dependencies?.[packageName] ?? dependencyVersion,
  });
};

const updateTsconfig = async ({
  packageName,
  packageSlug,
  paths,
  pendingWrites,
}) => {
  const tsconfig = await readJson(paths.tsconfigPath);
  tsconfig.compilerOptions.paths = sortObject({
    ...tsconfig.compilerOptions.paths,
    [packageName]: [`./packages/${packageSlug}/src/index.tsx`],
  });
  tsconfig.include = appendListItem(
    tsconfig.include,
    `packages/${packageSlug}/src`
  );
  queueWrite(
    pendingWrites,
    paths.tsconfigPath,
    `${JSON.stringify(tsconfig, null, 2)}\n`
  );
};

const updateViteConfig = async ({
  packageName,
  packageSlug,
  paths,
  pendingWrites,
}) => {
  const fileSource = await readFile(paths.viteConfigPath, "utf-8");
  const entry = `      "${packageName}": fileURLToPath(
        new URL("packages/${packageSlug}/src/index.tsx", import.meta.url)
      ),
`;
  queueWrite(
    pendingWrites,
    paths.viteConfigPath,
    insertBeforeNeedle(fileSource, entry, '      "@lenso/runtime-console-api":')
  );
};

const updateOxlintConfig = async ({ packageSlug, paths, pendingWrites }) => {
  const fileSource = await readFile(paths.oxlintConfigPath, "utf-8");
  const entry = `        "packages/${packageSlug}/src/**/*.{ts,tsx}",
`;
  queueWrite(
    pendingWrites,
    paths.oxlintConfigPath,
    insertBeforeNeedle(fileSource, entry, '        "vite.config.ts",')
  );
};

const updateManifestExports = async ({
  manifestName,
  packageName,
  paths,
  pendingWrites,
}) => {
  let fileSource = await readFile(paths.manifestExportsPath, "utf-8");
  fileSource = insertBeforeNeedle(
    fileSource,
    `import { ${manifestName} } from "${packageName}";
`,
    "export const consolePackageManifests"
  );
  fileSource = insertBeforeNeedle(
    fileSource,
    `  ${manifestName},\n`,
    "] as const;"
  );
  queueWrite(pendingWrites, paths.manifestExportsPath, fileSource);
};

const updateModuleExports = async ({
  manifestName,
  moduleName,
  packageName,
  paths,
  pendingWrites,
}) => {
  let fileSource = await readFile(paths.moduleExportsPath, "utf-8");
  fileSource = insertBeforeNeedle(
    fileSource,
    `import { ${manifestName}, ${moduleName} } from "${packageName}";
`,
    "import {"
  );
  fileSource = insertBeforeNeedle(
    fileSource,
    `  [consolePackageKey(${manifestName})]: ${moduleName},
`,
    "} satisfies ConsolePackageModuleExportsByKey;"
  );
  queueWrite(pendingWrites, paths.moduleExportsPath, fileSource);
};

const manifestNameFromModuleExport = (moduleName) =>
  moduleName.endsWith("Module")
    ? `${moduleName.slice(0, -"Module".length)}Manifest`
    : `${moduleName}Manifest`;

const uniqueConsolePackagePlanItems = (installPlan) => {
  const itemsByKey = new Map();
  for (const modulePlan of installPlan.modules ?? []) {
    for (const consolePackage of modulePlan.consolePackages ?? []) {
      if (!(consolePackage.packageName && consolePackage.exportName)) {
        continue;
      }
      const key = consolePackageKey({
        exportName: consolePackage.exportName,
        packageName: consolePackage.packageName,
      });
      itemsByKey.set(key, consolePackage);
    }
  }
  return [...itemsByKey.values()];
};

const applyConsolePackageInstallPlan = async ({ options }) => {
  const repoRoot = options.repoRoot
    ? path.resolve(options.repoRoot)
    : await findRepoRoot(process.cwd());
  const runtimeConsoleRoot = path.resolve(
    options.runtimeConsoleRoot ?? path.join(repoRoot, "apps/runtime-console")
  );
  const installPlanPath = path.resolve(
    options.installPlanFile ??
      path.join(repoRoot, ".lenso/console-package-install-plan.json")
  );
  const dependencyVersion = options.dependencyVersion ?? "latest";
  const installPlan = await readJson(installPlanPath);
  const paths = runtimeConsolePaths(runtimeConsoleRoot);
  const packageJson = await readJson(paths.packageJsonPath);
  let manifestExportsSource = await readFile(
    paths.manifestExportsPath,
    "utf-8"
  );
  let moduleExportsSource = await readFile(paths.moduleExportsPath, "utf-8");
  const pendingWrites = new Map();
  const planItems = uniqueConsolePackagePlanItems(installPlan);

  for (const planItem of planItems) {
    const manifestName = manifestNameFromModuleExport(planItem.exportName);
    updateRuntimeConsoleDependency({
      dependencyVersion,
      packageJson,
      packageName: planItem.packageName,
    });
    manifestExportsSource = insertBeforeNeedle(
      manifestExportsSource,
      `import { ${manifestName} } from "${planItem.packageName}";
`,
      "export const consolePackageManifests"
    );
    manifestExportsSource = insertBeforeNeedle(
      manifestExportsSource,
      `  ${manifestName},\n`,
      "] as const;"
    );
    moduleExportsSource = insertBeforeNeedle(
      moduleExportsSource,
      `import { ${manifestName}, ${planItem.exportName} } from "${planItem.packageName}";
`,
      "import {"
    );
    moduleExportsSource = insertBeforeNeedle(
      moduleExportsSource,
      `  [consolePackageKey(${manifestName})]: ${planItem.exportName},
`,
      "} satisfies ConsolePackageModuleExportsByKey;"
    );
  }

  queueWrite(
    pendingWrites,
    paths.packageJsonPath,
    `${JSON.stringify(packageJson, null, 2)}\n`
  );
  queueWrite(pendingWrites, paths.manifestExportsPath, manifestExportsSource);
  queueWrite(pendingWrites, paths.moduleExportsPath, moduleExportsSource);

  if (options.dryRun) {
    console.log("Console package install plan dry run:");
    for (const filePath of pendingWrites.keys()) {
      console.log(`- ${path.relative(repoRoot, filePath)}`);
    }
    return;
  }

  await writePendingFiles(pendingWrites);

  console.log(
    `Applied ${planItems.length} console package install plan item(s).`
  );
  console.log("Next steps:");
  console.log("- pnpm --dir apps/runtime-console install");
  console.log("- pnpm --dir apps/runtime-console check:console-packages");
  console.log("- just console-check");
};

const queuePackageFiles = ({
  area,
  capability,
  componentName,
  icon,
  label,
  manifestName,
  moduleId,
  moduleName,
  packageDir,
  packageName,
  packagePrivate,
  pendingWrites,
  route,
  runtimeConsoleApiVersion,
  registrySource,
  surfaceName,
}) => {
  const consoleSurfaceContract = {
    area,
    exportName: moduleName,
    icon,
    id: moduleId,
    label,
    navigation: {
      order: 10,
      workspace: {
        icon,
        id: moduleId,
        label,
      },
    },
    packageName,
    requiredCapabilities: [capability],
    route,
    source: registrySource,
    surfaceName,
    version: "workspace",
  };

  queueWrite(
    pendingWrites,
    path.join(packageDir, "package.json"),
    `${JSON.stringify(
      {
        exports: {
          ".": "./src/index.tsx",
        },
        name: packageName,
        peerDependencies: {
          "@lenso/runtime-console-api": runtimeConsoleApiVersion,
          react: "^19.1.0",
        },
        private: packagePrivate,
        scripts: {
          check: "pnpm test && pnpm typecheck",
          test: 'echo "console package smoke passed"',
          typecheck: 'echo "console package typecheck placeholder"',
        },
        type: "module",
        version: "0.1.0",
      },
      null,
      2
    )}\n`
  );

  queueWrite(
    pendingWrites,
    path.join(packageDir, "console-surface.json"),
    `${JSON.stringify(consoleSurfaceContract, null, 2)}\n`
  );

  queueWrite(
    pendingWrites,
    path.join(packageDir, "console-surface.rs"),
    `use platform_module::{ConsoleArea, ConsolePackage, ConsoleSurface};

ConsoleSurface {
    name: "${surfaceName}".to_owned(),
    label: "${label}".to_owned(),
    area: ConsoleArea::${rustConsoleArea(area)},
    route: "${route}".to_owned(),
    package: ConsolePackage {
        name: "${packageName}".to_owned(),
        export: "${moduleName}".to_owned(),
    },
    icon: Some("${icon}".to_owned()),
    required_capabilities: vec!["${capability}".to_owned()],
    navigation: Some(platform_module::ConsoleNavigation {
        workspace: platform_module::ConsoleWorkspaceRef {
            id: "${moduleId}".to_owned(),
            label: "${label}".to_owned(),
            icon: Some("${icon}".to_owned()),
        },
        group: None,
        order: Some(10),
    }),
}
`
  );

  queueWrite(
    pendingWrites,
    path.join(packageDir, "src/manifest.ts"),
    `import { defineConsolePackageManifest } from "@lenso/runtime-console-api";

import consoleSurface from "../console-surface.json";

const consoleSurfaceContract = consoleSurface as unknown as {
  readonly area: "${area}";
  readonly exportName: "${moduleName}";
  readonly icon: "${icon}";
  readonly id: "${moduleId}";
  readonly label: "${label}";
  readonly navigation: {
    readonly order: 10;
    readonly workspace: {
      readonly icon: "${icon}";
      readonly id: "${moduleId}";
      readonly label: "${label}";
    };
  };
  readonly packageName: "${packageName}";
  readonly requiredCapabilities: readonly ["${capability}"];
  readonly route: "${route}";
  readonly source: "${registrySource}";
  readonly surfaceName: "${surfaceName}";
  readonly version: "workspace";
};

export const ${manifestName} = defineConsolePackageManifest(
  consoleSurfaceContract
);
`
  );

  queueWrite(
    pendingWrites,
    path.join(packageDir, "src/page.tsx"),
    `export function ${componentName}() {
  return (
    <main className="flex min-h-screen flex-col gap-3 px-6 py-5">
      <header>
        <p className="font-medium text-muted-foreground text-xs uppercase tracking-normal">
          ${label}
        </p>
        <h1 className="font-semibold text-2xl text-foreground">${label}</h1>
      </header>
    </main>
  );
}
`
  );

  queueWrite(
    pendingWrites,
    path.join(packageDir, "src/index.tsx"),
    `import { defineConsoleModule } from "@lenso/runtime-console-api";

import { ${manifestName} } from "./manifest";
import { ${componentName} } from "./page";

export const ${moduleName} = defineConsoleModule({
  id: ${manifestName}.id,
  surfaces: [
    {
      area: ${manifestName}.area,
      component: ${componentName},
      icon: ${manifestName}.icon,
      label: ${manifestName}.label,
      navigation: ${manifestName}.navigation,
      path: ${manifestName}.route,
    },
  ],
});

export { ${manifestName} } from "./manifest";
export { ${componentName} } from "./page";
`
  );

  queueWrite(
    pendingWrites,
    path.join(packageDir, "src/index.test.tsx"),
    `import { describe, expect, test } from "vitest";

import { ${componentName}, ${manifestName}, ${moduleName} } from ".";

describe("${packageName}", () => {
  test("exports a console module manifest and route", () => {
    expect(${manifestName}).toMatchObject({
      exportName: "${moduleName}",
      id: "${moduleId}",
      packageName: "${packageName}",
      route: "${route}",
    });
    expect(${moduleName}).toMatchObject({
      id: ${manifestName}.id,
      surfaces: [
        {
          area: ${manifestName}.area,
          icon: ${manifestName}.icon,
          label: ${manifestName}.label,
          path: ${manifestName}.route,
        },
      ],
    });
    expect(${moduleName}.surfaces[0]?.component).toBe(${componentName});
  });
});
`
  );
};

const buildConsolePackageContext = ({ options, runtimeConsoleRoot }) => {
  const paths = runtimeConsolePaths(runtimeConsoleRoot);
  const moduleId = slugify(options.moduleId);
  const packageSlug = slugify(options.packageSlug ?? `${moduleId}-console`);
  const packageName =
    options.packageName ?? `${options.packageScope ?? "@lenso"}/${packageSlug}`;
  const area = options.area ?? "data";
  const label = options.label ?? titleCase(moduleId);
  const route = options.route ?? `/${area}/${moduleId}`;
  const registrySource = options.source ?? "installed";
  const icon = options.icon ?? defaultIcon(area);
  const capability = options.capability ?? `${moduleId}.read`;
  const surfaceName = options.surfaceName ?? moduleId;
  const exportStem = exportStemFromPackageSlug(packageSlug);
  const manifestName = `${exportStem}Manifest`;
  const moduleName = `${exportStem}Module`;
  const componentName = `${pascalCase(moduleId)}ConsolePage`;
  const packageDir = path.join(runtimeConsoleRoot, "packages", packageSlug);

  return {
    area,
    capability,
    componentName,
    icon,
    label,
    manifestName,
    moduleId,
    moduleName,
    packageDir,
    packageName,
    packagePrivate: options.packagePrivate ?? true,
    packageSlug,
    paths,
    registrySource,
    route,
    runtimeConsoleApiVersion: options.runtimeConsoleApiVersion ?? "workspace:*",
    surfaceName,
  };
};

const queueRemoteConsolePackageFiles = ({ packageContext, pendingWrites }) => {
  queuePackageFiles({ ...packageContext, pendingWrites });
};

const remoteManifestJson = ({ packageContext }) => ({
  admin: null,
  capabilities: [packageContext.capability],
  console: [
    {
      area: packageContext.area,
      icon: packageContext.icon,
      label: packageContext.label,
      name: packageContext.surfaceName,
      navigation: {
        order: 10,
        workspace: {
          icon: packageContext.icon,
          id: packageContext.moduleId,
          label: packageContext.label,
        },
      },
      package: {
        export: packageContext.moduleName,
        name: packageContext.packageName,
      },
      required_capabilities: [packageContext.capability],
      route: packageContext.route,
    },
  ],
  http_routes: [],
  name: packageContext.moduleId,
  runtime: {
    functions: [],
  },
  source: "remote",
  version: "0.1.0",
});

const remoteCatalogEntryJson = ({ packageContext }) => ({
  baseUrl: "https://example.com/lenso/module/v1",
  consolePackages: [
    {
      exportName: packageContext.moduleName,
      packageName: packageContext.packageName,
      route: packageContext.route,
    },
  ],
  manifestReference: "https://example.com/lenso/module/v1/manifest",
  name: packageContext.moduleId,
  source: "remote",
  summary: `${packageContext.label} workspace and operations`,
  version: "0.1.0",
});

const remotePackageReadme = ({
  moduleId,
  packageRootName,
}) => `# ${titleCase(moduleId)}

Remote Lenso module package scaffold.

## Shape

- \`lenso.module.json\`: install-time module manifest.
- \`catalog-entry.json\`: optional local catalog entry for discovery.
- \`backend/\`: remote module backend implementation.
- \`console/\`: optional Runtime Console package.
- \`contracts/\`: module-owned event and runtime-function contracts.

## Local

\`\`\`sh
pnpm dev
pnpm smoke
pnpm check
\`\`\`

## Install

Expose the remote module protocol from a stable base URL such as:

\`\`\`text
GET https://example.com/lenso/module/v1/manifest
\`\`\`

Use \`catalog-entry.json\` as the local discovery record, or add the manifest
URL directly:

\`\`\`sh
lenso module catalog add https://example.com/lenso/module/v1/manifest
\`\`\`

Then install it into a host project:

\`\`\`sh
lenso module catalog add https://example.com/lenso/module/v1/manifest
lenso module add https://example.com/lenso/module/v1/manifest
lenso console-package apply-plan
pnpm --dir apps/runtime-console install
\`\`\`

If the manifest is inspected from a local file, provide the runtime base URL:

\`\`\`sh
lenso module add ./lenso.module.json --base-url https://example.com/lenso/module/v1
lenso console-package apply-plan
\`\`\`

This scaffold lives in \`${packageRootName}\` and should stay separate from a
host application's linked \`modules/\` workspace.
`;

const remoteBackendReadme = ({ moduleId }) => `# Remote module backend

The generated Node server exposes the ${moduleId} manifest at:

\`\`\`text
GET /lenso/module/v1/manifest
\`\`\`

Run it locally:

\`\`\`sh
cd backend
pnpm install
pnpm dev
\`\`\`

Replace \`src/server.mjs\` with the language or framework you prefer as the
module grows.

The backend should expose the remote module protocol expected by
\`platform-module-remote\`, including a stable manifest endpoint and any
declared schema-admin, action, HTTP proxy, or runtime-function endpoints.

The host owns auth, capability enforcement, proxy policy, runtime queues,
retries, Runtime Stories, and Technical Operations records.
`;

const remoteBackendPackageJson = ({ moduleId }) =>
  `${JSON.stringify(
    {
      dependencies: {
        "@lenso/remote-module-kit": "^0.1.0",
      },
      name: `${moduleId}-remote-backend`,
      private: true,
      scripts: {
        check: "node src/smoke.mjs",
        dev: "node src/server.mjs",
        smoke: "node src/smoke.mjs",
        start: "node src/server.mjs",
      },
      type: "module",
      version: "0.1.0",
    },
    null,
    2
  )}\n`;

const remoteBackendServer = ({ packageContext }) => `import {
  defineRemoteModule,
  serveRemoteModule,
} from "@lenso/remote-module-kit";

const module = defineRemoteModule({
  capabilities: ["${packageContext.capability}"],
  console: [
    {
      area: "${packageContext.area}",
      icon: "${packageContext.icon}",
      label: "${packageContext.label}",
      name: "${packageContext.surfaceName}",
      navigation: {
        order: 10,
        workspace: {
          icon: "${packageContext.icon}",
          id: "${packageContext.moduleId}",
          label: "${packageContext.label}",
        },
      },
      package: {
        export: "${packageContext.moduleName}",
        name: "${packageContext.packageName}",
      },
      required_capabilities: ["${packageContext.capability}"],
      route: "${packageContext.route}",
    },
  ],
  name: "${packageContext.moduleId}",
  version: "0.1.0",
});

await serveRemoteModule(module, {
  port: Number(process.env.PORT ?? 4100),
  onReady: ({ manifestUrl }) => {
    console.log("${packageContext.moduleId} manifest: " + manifestUrl);
  },
});
`;

const remoteBackendSmoke = ({
  moduleId,
}) => `import { spawn } from "node:child_process";

const childProcess = spawn(process.execPath, ["src/server.mjs"], {
  env: { ...process.env, PORT: "0" },
  stdio: ["ignore", "pipe", "inherit"],
});

const timeout = setTimeout(() => childProcess.kill(), 3000);

try {
  let manifestUrl = "";
  for await (const chunk of childProcess.stdout) {
    manifestUrl = String(chunk).match(new RegExp("http://\\\\S+", "u"))?.[0] ?? "";
    if (manifestUrl) {
      break;
    }
  }

  if (!manifestUrl) {
    throw new Error("manifest URL was not printed");
  }

  const manifest = await fetch(manifestUrl).then((response) => response.json());
  if (manifest.name !== "${moduleId}" || manifest.source !== "remote") {
    throw new Error("manifest response did not match ${moduleId}");
  }

  console.log("${moduleId} backend smoke passed");
} finally {
  clearTimeout(timeout);
  childProcess.kill();
}
`;

const remoteRootPackageJson = ({ moduleId }) =>
  `${JSON.stringify(
    {
      name: `lenso-${moduleId}`,
      private: true,
      scripts: {
        check: "pnpm --dir backend check && pnpm --dir console check",
        dev: "pnpm --dir backend dev",
        smoke: "pnpm --dir backend smoke",
      },
      type: "module",
      version: "0.1.0",
    },
    null,
    2
  )}\n`;

const remoteContractsReadme = () => `# Module-owned contracts

Keep event and runtime-function JSON Schema contracts here.

The host may validate these before installing or enabling a remote module.
`;

const queueRemoteModuleFiles = ({
  packageContext,
  packageRoot,
  packageRootName,
  pendingWrites,
}) => {
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "lenso.module.json"),
    `${JSON.stringify(remoteManifestJson({ packageContext }), null, 2)}\n`
  );
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "catalog-entry.json"),
    `${JSON.stringify(remoteCatalogEntryJson({ packageContext }), null, 2)}\n`
  );
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "package.json"),
    remoteRootPackageJson({ moduleId: packageContext.moduleId })
  );
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "README.md"),
    remotePackageReadme({
      moduleId: packageContext.moduleId,
      packageRootName,
    })
  );
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "backend/README.md"),
    remoteBackendReadme({ moduleId: packageContext.moduleId })
  );
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "backend/package.json"),
    remoteBackendPackageJson({ moduleId: packageContext.moduleId })
  );
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "backend/src/server.mjs"),
    remoteBackendServer({ packageContext })
  );
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "backend/src/smoke.mjs"),
    remoteBackendSmoke({ moduleId: packageContext.moduleId })
  );
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "backend/openapi.yaml"),
    `openapi: 3.1.0
info:
  title: ${packageContext.label} Remote Module
  version: 0.1.0
paths: {}
`
  );
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "contracts/README.md"),
    remoteContractsReadme()
  );
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "contracts/events/.gitkeep"),
    ""
  );
  queueWrite(
    pendingWrites,
    path.join(packageRoot, "contracts/runtime-functions/.gitkeep"),
    ""
  );
  queueRemoteConsolePackageFiles({ packageContext, pendingWrites });
};

const writePendingFiles = async (pendingWrites) => {
  for (const [filePath, content] of pendingWrites) {
    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(filePath, content);
  }
};

const moduleCargoToml = ({ moduleId }) => `[package]
name = "${moduleId}"
version = "0.1.0"
edition.workspace = true
license.workspace = true
publish.workspace = true
rust-version.workspace = true

[dependencies]
platform-core.workspace = true
platform-module.workspace = true

[lints]
workspace = true
`;

const moduleLib = () => `pub mod module;
`;

const moduleManifestImports = (consoleSurface) =>
  consoleSurface
    ? "use platform_module::{ConsoleArea, ConsolePackage, ConsoleSurface, LinkedBinding, Module, ModuleManifest};"
    : "use platform_module::{LinkedBinding, Module, ModuleManifest};";

const moduleManifestBuilder = ({ consoleSurface, moduleId }) => {
  if (!consoleSurface) {
    return `ModuleManifest::builder("${moduleId}").build()`;
  }
  return `ModuleManifest::builder("${moduleId}")
        .capabilities(vec!["${consoleSurface.capability}".to_owned()])
        .console(vec![ConsoleSurface {
            name: "${consoleSurface.surfaceName}".to_owned(),
            label: "${consoleSurface.label}".to_owned(),
            area: ConsoleArea::${rustConsoleArea(consoleSurface.area)},
            route: "${consoleSurface.route}".to_owned(),
            package: ConsolePackage {
                name: "${consoleSurface.packageName}".to_owned(),
                export: "${consoleSurface.moduleName}".to_owned(),
            },
            icon: Some("${consoleSurface.icon}".to_owned()),
            required_capabilities: vec!["${consoleSurface.capability}".to_owned()],
            navigation: Some(platform_module::ConsoleNavigation {
                workspace: platform_module::ConsoleWorkspaceRef {
                    id: "${moduleId}".to_owned(),
                    label: "${consoleSurface.label}".to_owned(),
                    icon: Some("${consoleSurface.icon}".to_owned()),
                },
                group: None,
                order: Some(10),
            }),
        }])
        .build()`;
};

const moduleManifest = ({
  consoleSurface,
  moduleId,
}) => `use platform_core::AppContext;
${moduleManifestImports(consoleSurface)}

/// Context-free manifest: serializable metadata only.
pub fn manifest() -> ModuleManifest {
    ${moduleManifestBuilder({ consoleSurface, moduleId })}
}

/// The loaded module: manifest + linked behavior.
pub fn module(_ctx: &AppContext) -> Module {
    Module::linked(manifest(), LinkedBinding::builder().build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_uses_module_name() {
        assert_eq!(manifest().name, "${moduleId}");
    }
}
`;

const updateWorkspaceCargoToml = async ({
  moduleCrate,
  moduleId,
  paths,
  pendingWrites,
}) => {
  let fileSource = await readFile(paths.cargoTomlPath, "utf-8");
  fileSource = insertBeforeFirstNeedle(
    fileSource,
    `    "modules/${moduleId}",\n`,
    ['    "tools/', "]\n\n[workspace.package]"]
  );
  fileSource = insertBeforeFirstNeedle(
    fileSource,
    `${moduleCrate} = { path = "modules/${moduleId}" }\n`,
    ["generate-contracts =", "arch-check =", "remote-module-example ="]
  );
  queueWrite(pendingWrites, paths.cargoTomlPath, fileSource);
};

const updateAppBootstrapCargoToml = async ({
  moduleCrate,
  paths,
  pendingWrites,
}) => {
  const fileSource = await readFile(paths.appBootstrapCargoTomlPath, "utf-8");
  queueWrite(
    pendingWrites,
    paths.appBootstrapCargoTomlPath,
    insertBeforeFirstNeedle(fileSource, `${moduleCrate}.workspace = true\n`, [
      "serde_json.workspace",
      "tracing.workspace",
      "\n[dev-dependencies]",
    ])
  );
};

const updateAppBootstrapLib = async ({
  moduleCrate,
  moduleId,
  paths,
  pendingWrites,
}) => {
  const fileSource = await readFile(paths.appBootstrapLibPath, "utf-8");
  const entry = `    LinkedModuleEntry {
        module_name: "${moduleId}",
        manifest: ${moduleCrate}::module::manifest,
        load: ${moduleCrate}::module::module,
        http_binding: None,
    },
`;
  queueWrite(
    pendingWrites,
    paths.appBootstrapLibPath,
    insertIntoLinkedModuleEntries(fileSource, entry)
  );
};

const queueModuleFiles = ({
  consoleSurface,
  moduleDir,
  moduleId,
  pendingWrites,
}) => {
  queueWrite(
    pendingWrites,
    path.join(moduleDir, "Cargo.toml"),
    moduleCargoToml({ moduleId })
  );
  queueWrite(pendingWrites, path.join(moduleDir, "src/lib.rs"), moduleLib());
  queueWrite(
    pendingWrites,
    path.join(moduleDir, "src/module.rs"),
    moduleManifest({ consoleSurface, moduleId })
  );
};

const createModule = async ({ options }) => {
  if (options.remote) {
    await createRemoteModule({ options });
    return;
  }

  const repoRoot = options.repoRoot
    ? path.resolve(options.repoRoot)
    : await findRepoRoot(process.cwd());
  const moduleId = slugify(options.moduleId);
  if (!moduleId) {
    throw new Error("Module id is required");
  }
  const moduleCrate = snakeCase(moduleId);
  const moduleDir = path.join(repoRoot, "modules", moduleId);
  const consoleRuntimeRoot = path.resolve(
    options.runtimeConsoleRoot ?? path.join(repoRoot, "apps/runtime-console")
  );
  const consoleSurface = options.withConsole
    ? buildConsolePackageContext({
        options: { ...options, moduleId },
        runtimeConsoleRoot: consoleRuntimeRoot,
      })
    : undefined;

  if (await pathExists(moduleDir)) {
    throw new Error(`Module directory already exists: modules/${moduleId}`);
  }

  const paths = repoPaths(repoRoot);
  const pendingWrites = new Map();
  const moduleContext = {
    consoleSurface,
    moduleCrate,
    moduleDir,
    moduleId,
    paths,
    pendingWrites,
  };

  queueModuleFiles(moduleContext);
  await updateWorkspaceCargoToml(moduleContext);
  await updateAppBootstrapCargoToml(moduleContext);
  await updateAppBootstrapLib(moduleContext);

  if (options.dryRun) {
    console.log("Module dry run:");
    for (const filePath of pendingWrites.keys()) {
      console.log(`- ${path.relative(repoRoot, filePath)}`);
    }
    if (options.withConsole) {
      await createConsolePackage({
        defaultRuntimeConsoleRoot: consoleRuntimeRoot,
        options: { ...options, moduleId },
      });
    }
    return;
  }

  await writePendingFiles(pendingWrites);
  if (options.withConsole) {
    await createConsolePackage({
      defaultRuntimeConsoleRoot: consoleRuntimeRoot,
      options: { ...options, moduleId },
    });
  }

  console.log(`Created module ${moduleId}.`);
  console.log("Next steps:");
  console.log(`- cargo test --locked -p ${moduleCrate}`);
  console.log("- just rust-check");
  console.log("- just arch-check");
};

const createRemoteModule = async ({ options }) => {
  const moduleId = slugify(options.moduleId);
  if (!moduleId) {
    throw new Error("Module id is required");
  }
  const outputRoot = path.resolve(options.outputDir ?? process.cwd());
  const packageRootName = slugify(options.packageRoot ?? `lenso-${moduleId}`);
  const packageRoot = path.join(outputRoot, packageRootName);
  const packageContext = buildConsolePackageContext({
    options: {
      ...options,
      moduleId,
      packageName:
        options.packageName ??
        `${options.packageScope ?? "@vendor"}/lenso-${moduleId}-console`,
      packagePrivate: false,
      packageSlug: `${moduleId}-console`,
      runtimeConsoleApiVersion: "^0.1.0",
      source: options.source ?? "installed",
    },
    runtimeConsoleRoot: packageRoot,
  });
  packageContext.packageDir = path.join(packageRoot, "console");

  if (await pathExists(packageRoot)) {
    throw new Error(`Remote module package already exists: ${packageRoot}`);
  }

  const pendingWrites = new Map();
  queueRemoteModuleFiles({
    packageContext,
    packageRoot,
    packageRootName,
    pendingWrites,
  });

  if (options.dryRun) {
    console.log("Remote module dry run:");
    for (const filePath of pendingWrites.keys()) {
      console.log(`- ${path.relative(outputRoot, filePath)}`);
    }
    return;
  }

  await writePendingFiles(pendingWrites);

  console.log(`Created remote module package ${packageRootName}.`);
  console.log("Next steps:");
  console.log(`- pnpm --dir ${packageRootName}/backend dev`);
  console.log(
    `- lenso module catalog add http://127.0.0.1:4100/lenso/module/v1/manifest`
  );
  console.log(
    `- lenso module add http://127.0.0.1:4100/lenso/module/v1/manifest`
  );
  console.log("- publish or install the console package");
  console.log("- lenso console-package apply-plan");
  console.log("- pnpm --dir apps/runtime-console install");
};

const validateRemoteModuleManifest = (manifest) => {
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest)) {
    throw new Error("Remote module manifest must be a JSON object");
  }
  if (typeof manifest.name !== "string" || !manifest.name.trim()) {
    throw new Error("Remote module manifest name is required");
  }
  if (typeof manifest.version !== "string" || !manifest.version.trim()) {
    throw new Error("Remote module manifest version is required");
  }
  if (manifest.source !== "remote") {
    throw new Error("Remote module manifest source must be remote");
  }
  if (!Array.isArray(manifest.capabilities)) {
    throw new TypeError("Remote module manifest capabilities must be an array");
  }
  if (!Array.isArray(manifest.console)) {
    throw new TypeError("Remote module manifest console must be an array");
  }
  return {
    name: manifest.name.trim(),
    version: manifest.version,
  };
};

const trimTrailingSlash = (value) => value.replaceAll(/\/+$/gu, "");

const deriveRemoteBaseUrl = ({ baseUrl, manifestReference }) => {
  if (baseUrl) {
    return trimTrailingSlash(baseUrl);
  }
  if (
    manifestReference.startsWith("http://") ||
    manifestReference.startsWith("https://")
  ) {
    const url = new URL(manifestReference);
    if (url.pathname.endsWith("/manifest")) {
      url.pathname = url.pathname.slice(0, -"/manifest".length);
      url.search = "";
      url.hash = "";
      return trimTrailingSlash(url.toString());
    }
  }
  throw new Error(
    "Remote module base URL is required unless the manifest URL ends with /manifest"
  );
};

const updateRemoteModulesEnv = async ({ envFilePath, moduleName, baseUrl }) => {
  const source = await readTextIfExists(envFilePath);
  const remoteModulesLine = source
    .split("\n")
    .find((line) => line.startsWith("REMOTE_MODULES="));
  const currentValue = remoteModulesLine?.slice("REMOTE_MODULES=".length) ?? "";
  const entries = parseRemoteModuleEntries(currentValue).filter(
    (entry) => entry.name !== moduleName
  );
  entries.push({ baseUrl, name: moduleName });
  return upsertEnvValue(
    source,
    "REMOTE_MODULES",
    formatRemoteModuleEntries(entries)
  );
};

const remoteModuleConsolePackagePlans = ({ manifest, moduleName }) =>
  manifest.console
    .map((surface) => ({
      exportName: surface.package?.export,
      packageName: surface.package?.name,
      route: surface.route ?? "-",
      surfaceLabel: surface.label ?? surface.name ?? "-",
      surfaceName: surface.name ?? "-",
    }))
    .filter((surface) => surface.packageName && surface.exportName)
    .map((surface) => {
      const packageReference = {
        exportName: surface.exportName,
        packageName: surface.packageName,
      };
      return {
        command: `pnpm --dir apps/runtime-console add ${surface.packageName}`,
        exportName: surface.exportName,
        key: consolePackageKey(packageReference),
        packageName: surface.packageName,
        reason: `${moduleName} / ${surface.surfaceLabel} / ${surface.route}`,
        requestedByModule: moduleName,
        route: surface.route,
        status: "requires_manual_install",
        surfaceLabel: surface.surfaceLabel,
        surfaceName: surface.surfaceName,
      };
    });

const updateConsolePackageInstallPlan = async ({
  baseUrl,
  installPlanPath,
  manifest,
  manifestReference,
  moduleName,
}) => {
  const source = await readTextIfExists(installPlanPath);
  const currentPlan = source
    ? JSON.parse(source)
    : {
        modules: [],
        version: 1,
      };
  const modules = (currentPlan.modules ?? []).filter(
    (module) => module.moduleName !== moduleName
  );
  modules.push({
    baseUrl,
    consolePackages: remoteModuleConsolePackagePlans({
      manifest,
      moduleName,
    }),
    manifestReference,
    moduleName,
    restartRequired: true,
  });
  return `${JSON.stringify({ modules, version: 1 }, null, 2)}\n`;
};

const readModuleCatalog = async (catalogFilePath) => {
  const source = await readTextIfExists(catalogFilePath);
  if (!source) {
    return { modules: [], version: 1 };
  }
  const catalog = JSON.parse(source);
  if (!catalog || typeof catalog !== "object" || Array.isArray(catalog)) {
    throw new Error("Module catalog must be a JSON object");
  }
  if (!Array.isArray(catalog.modules)) {
    throw new TypeError("Module catalog modules must be an array");
  }
  return {
    modules: catalog.modules,
    version: Number.isInteger(catalog.version) ? catalog.version : 1,
  };
};

const moduleCatalogEntryFromManifest = ({
  baseUrl,
  manifest,
  manifestReference,
  summary,
}) => ({
  baseUrl,
  consolePackages: manifest.console
    .map((surface) => ({
      exportName: surface.package?.export,
      packageName: surface.package?.name,
      route: surface.route,
    }))
    .filter((item) => item.packageName && item.exportName),
  manifestReference,
  name: manifest.name.trim(),
  source: "remote",
  summary: summary ?? manifest.summary ?? "-",
  version: manifest.version.trim(),
});

const addModuleCatalogEntry = async ({ manifestReference, options }) => {
  const repoRoot = options.repoRoot
    ? path.resolve(options.repoRoot)
    : await findRepoRoot(process.cwd());
  const catalogFilePath = path.resolve(
    options.catalogFile ?? path.join(repoRoot, ".lenso/module-catalog.json")
  );
  const manifest = await readJsonFromReference(manifestReference);
  const remoteModule = validateRemoteModuleManifest(manifest);
  const baseUrl = deriveRemoteBaseUrl({
    baseUrl: options.baseUrl,
    manifestReference,
  });
  const catalog = await readModuleCatalog(catalogFilePath);
  const modules = catalog.modules.filter(
    (entry) => entry.name !== remoteModule.name
  );
  modules.push(
    moduleCatalogEntryFromManifest({
      baseUrl,
      manifest,
      manifestReference,
      summary: options.summary,
    })
  );
  const nextCatalog = `${JSON.stringify({ modules, version: 1 }, null, 2)}\n`;

  if (options.dryRun) {
    console.log("Module catalog dry run:");
    console.log(`- ${path.relative(repoRoot, catalogFilePath)}`);
    console.log(`- ${remoteModule.name} ${remoteModule.version}`);
    return;
  }

  await mkdir(path.dirname(catalogFilePath), { recursive: true });
  await writeFile(catalogFilePath, nextCatalog);

  console.log(`Added ${remoteModule.name} to module catalog.`);
  console.log("Updated:");
  console.log(`- ${path.relative(repoRoot, catalogFilePath)}`);
  console.log("Install:");
  console.log(`- lenso module add ${manifestReference}`);
};

const addRemoteModule = async ({ manifestReference, options }) => {
  const repoRoot = options.repoRoot
    ? path.resolve(options.repoRoot)
    : await findRepoRoot(process.cwd());
  const envFilePath = path.resolve(
    options.envFile ?? path.join(repoRoot, ".env")
  );
  const installPlanPath = path.resolve(
    options.installPlanFile ??
      path.join(repoRoot, ".lenso/console-package-install-plan.json")
  );
  const manifest = await readJsonFromReference(manifestReference);
  const remoteModule = validateRemoteModuleManifest(manifest);
  const baseUrl = deriveRemoteBaseUrl({
    baseUrl: options.baseUrl,
    manifestReference,
  });
  const envFile = await updateRemoteModulesEnv({
    baseUrl,
    envFilePath,
    moduleName: remoteModule.name,
  });
  const installPlan = await updateConsolePackageInstallPlan({
    baseUrl,
    installPlanPath,
    manifest,
    manifestReference,
    moduleName: remoteModule.name,
  });

  if (options.dryRun) {
    console.log("Remote module install dry run:");
    console.log(`- ${path.relative(repoRoot, envFilePath)}`);
    console.log(`- ${path.relative(repoRoot, installPlanPath)}`);
    console.log(`- ${remoteModule.name}=${baseUrl}`);
    return;
  }

  await mkdir(path.dirname(envFilePath), { recursive: true });
  await writeFile(envFilePath, envFile);
  await mkdir(path.dirname(installPlanPath), { recursive: true });
  await writeFile(installPlanPath, installPlan);

  console.log(`Added remote module ${remoteModule.name}.`);
  console.log("Updated:");
  console.log(`- ${path.relative(repoRoot, envFilePath)}`);
  console.log(`- ${path.relative(repoRoot, installPlanPath)}`);
  console.log("Next steps:");
  console.log("- lenso console-package apply-plan");
  console.log("- pnpm --dir apps/runtime-console install");
  console.log("- restart the API and worker");
};

const createConsolePackage = async ({ defaultRuntimeConsoleRoot, options }) => {
  const runtimeConsoleRoot = path.resolve(
    options.runtimeConsoleRoot ?? defaultRuntimeConsoleRoot ?? process.cwd()
  );
  const packageContext = buildConsolePackageContext({
    options,
    runtimeConsoleRoot,
  });

  if (await pathExists(packageContext.packageDir)) {
    throw new Error(
      `Console package directory already exists: ${relativePath(
        runtimeConsoleRoot,
        packageContext.packageDir
      )}`
    );
  }

  const pendingWrites = new Map();
  packageContext.pendingWrites = pendingWrites;

  queuePackageFiles(packageContext);
  await updatePackageJson(packageContext);
  await updateTsconfig(packageContext);
  await updateViteConfig(packageContext);
  await updateOxlintConfig(packageContext);
  await updateManifestExports(packageContext);
  await updateModuleExports(packageContext);

  if (options.dryRun) {
    console.log("Console package dry run:");
    for (const filePath of pendingWrites.keys()) {
      console.log(`- ${relativePath(runtimeConsoleRoot, filePath)}`);
    }
    return;
  }

  await writePendingFiles(pendingWrites);

  console.log(`Created ${packageContext.packageName}.`);
  console.log("Next steps:");
  console.log(
    `- Copy ${packageContext.packageSlug}/console-surface.rs into the Rust module manifest`
  );
  console.log(
    `- Keep navigation.workspace.id="${packageContext.moduleId}" so the module owns its workspace`
  );
  console.log("- Omit navigation only for host System surfaces");
  console.log("- pnpm install --lockfile-only");
  console.log("- pnpm check:console-packages");
  console.log("- just console-check");
};

const addSharedCreateOptions = (command) =>
  command
    .option("--repo-root <path>", "Lenso host repository root")
    .option("--output-dir <path>", "directory for standalone remote packages")
    .option("--runtime-console-root <path>", "Runtime Console app root")
    .option("--area <name>", "console surface area")
    .option("--label <label>", "display label")
    .option("--route <route>", "console route")
    .option("--capability <capability>", "required capability")
    .option("--icon <icon>", "Lucide icon name")
    .option("--source <source>", "console package install source")
    .option("--remote", "create a standalone remote module package")
    .option("--with-console", "create a matching Runtime Console package")
    .option("--package-slug <slug>", "console package slug")
    .option("--package-scope <scope>", "console package npm scope")
    .option("--package-name <name>", "full console package name")
    .option("--surface-name <name>", "console surface name")
    .option("--package-root <name>", "remote package root directory")
    .option("--dry-run", "print files without writing them");

const addRemoteModuleOptions = (command) =>
  command
    .option("--repo-root <path>", "Lenso host repository root")
    .option("--env-file <path>", "env file to update")
    .option("--install-plan-file <path>", "console package install plan file")
    .option("--base-url <url>", "remote module base URL")
    .option("--dry-run", "print install changes without writing them");

const addModuleCatalogOptions = (command) =>
  command
    .option("--repo-root <path>", "Lenso host repository root")
    .option("--catalog-file <path>", "module catalog file to update")
    .option("--base-url <url>", "remote module base URL")
    .option("--summary <text>", "catalog summary text")
    .option("--dry-run", "print catalog changes without writing them");

const addApplyPlanOptions = (command) =>
  command
    .option("--repo-root <path>", "Lenso host repository root")
    .option("--runtime-console-root <path>", "Runtime Console app root")
    .option("--install-plan-file <path>", "console package install plan file")
    .option(
      "--dependency-version <version>",
      "dependency version to write when the package is not already declared"
    )
    .option("--dry-run", "print install plan changes without writing them");

const createProgram = ({ defaultRuntimeConsoleRoot } = {}) => {
  const program = new Command();
  program
    .name("lenso")
    .description("Lenso module and Runtime Console package tooling")
    .exitOverride()
    .showHelpAfterError();

  const moduleCommand = program
    .command("module")
    .description("create and manage Lenso modules")
    .addHelpText(
      "after",
      `
Remote module install:
  lenso module add <manifest-url>
  lenso module marketplace install <manifest-url>
  lenso console-package apply-plan
`
    );
  addSharedCreateOptions(
    moduleCommand
      .command("create <moduleId>")
      .description("create a linked or remote module scaffold")
  ).action(async (moduleId, options) => {
    await createModule({ options: { ...options, moduleId } });
  });
  addRemoteModuleOptions(
    moduleCommand
      .command("add <manifestReference>")
      .description("add a configured remote module source")
  ).action(async (manifestReference, options) => {
    await addRemoteModule({ manifestReference, options });
  });
  const catalogCommand = moduleCommand
    .command("catalog")
    .description("manage a local module catalog");
  addModuleCatalogOptions(
    catalogCommand
      .command("add <manifestReference>")
      .description("add a remote module manifest to the local catalog")
  ).action(async (manifestReference, options) => {
    await addModuleCatalogEntry({ manifestReference, options });
  });
  const marketplaceCommand = moduleCommand
    .command("marketplace")
    .description("install remote modules");
  addRemoteModuleOptions(
    marketplaceCommand
      .command("install <manifestReference>")
      .description("install a remote module from its manifest")
  ).action(async (manifestReference, options) => {
    await addRemoteModule({ manifestReference, options });
  });

  const consolePackageCommand = program
    .command("console-package")
    .description("create Runtime Console package scaffolds");
  addSharedCreateOptions(
    consolePackageCommand
      .command("create <moduleId>")
      .description("create a Runtime Console package scaffold")
  ).action(async (moduleId, options) => {
    await createConsolePackage({
      defaultRuntimeConsoleRoot,
      options: { ...options, moduleId },
    });
  });
  addApplyPlanOptions(
    consolePackageCommand
      .command("apply-plan")
      .description("apply a console package install plan")
  ).action(async (options) => {
    await applyConsolePackageInstallPlan({ options });
  });

  addSharedCreateOptions(
    program
      .command("create <moduleId>")
      .description("create a Runtime Console package scaffold")
  ).action(async (moduleId, options) => {
    await createConsolePackage({
      defaultRuntimeConsoleRoot,
      options: { ...options, moduleId },
    });
  });

  return program;
};

export const runConsolePackageCli = async (
  args,
  { defaultRuntimeConsoleRoot } = {}
) => {
  const normalizedArgs = args.filter((arg) => arg !== "--");
  const program = createProgram({ defaultRuntimeConsoleRoot });
  if (normalizedArgs.length === 0) {
    program.outputHelp();
    return 1;
  }

  try {
    await program.parseAsync(normalizedArgs, { from: "user" });
    return 0;
  } catch (error) {
    if (typeof error.exitCode === "number") {
      return error.exitCode;
    }
    throw error;
  }
};

const isCliEntry = () =>
  process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename;

if (isCliEntry()) {
  try {
    const exitCode = await runConsolePackageCli(process.argv.slice(2));
    process.exit(exitCode);
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}
