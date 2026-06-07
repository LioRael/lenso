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

const remotePackageReadme = ({
  moduleId,
  packageRootName,
}) => `# ${titleCase(moduleId)}

Remote Lenso module package scaffold.

## Shape

- \`lenso.module.json\`: install-time module manifest.
- \`backend/\`: remote module backend implementation.
- \`console/\`: optional Runtime Console package.
- \`contracts/\`: module-owned event and runtime-function contracts.

## Install

Expose the remote module protocol from a stable base URL such as:

\`\`\`text
GET https://example.com/lenso/module/v1/manifest
\`\`\`

Then install it into a host project:

\`\`\`sh
lenso module add https://example.com/lenso/module/v1/manifest
lenso console-package apply-plan
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

Implement the ${moduleId} backend in the language or framework you prefer.

The backend should expose the remote module protocol expected by
\`platform-module-remote\`, including a stable manifest endpoint and any
declared schema-admin, action, HTTP proxy, or runtime-function endpoints.

The host owns auth, capability enforcement, proxy policy, runtime queues,
retries, Runtime Stories, and Technical Operations records.
`;

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
  console.log("- expose lenso.module.json from a stable module URL");
  console.log("- publish or install the console package");
  console.log("- lenso module add <manifest-url>");
  console.log("- lenso console-package apply-plan");
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

const remoteModulesFromEnvFile = async (envFilePath) => {
  const source = await readTextIfExists(envFilePath);
  const remoteModulesLine = source
    .split("\n")
    .find((line) => line.startsWith("REMOTE_MODULES="));
  const currentValue = remoteModulesLine?.slice("REMOTE_MODULES=".length) ?? "";
  return parseRemoteModuleEntries(currentValue);
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
  });
  return `${JSON.stringify({ modules, version: 1 }, null, 2)}\n`;
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
  console.log("Next steps:");
  console.log(
    `- review ${path.relative(repoRoot, installPlanPath)} for console package install commands`
  );
  console.log("- restart the API and worker so REMOTE_MODULES is reloaded");
  console.log(
    "- open the Runtime Console module registry and workspace switcher to verify the remote source"
  );
};

const registryConsolePackageKey = ({ exportName, packageName }) =>
  packageName && exportName ? `${packageName}#${exportName}` : null;

const consolePackagesFromRegistryEntry = (entry) =>
  (entry.consolePackages ?? [])
    .map((consolePackage) =>
      registryConsolePackageKey({
        exportName: consolePackage.exportName,
        packageName: consolePackage.packageName,
      })
    )
    .filter(Boolean);

const consolePackagesFromManifest = (manifest) =>
  remoteModuleConsolePackagePlans({
    manifest,
    moduleName: manifest.name,
  }).map((consolePackage) => consolePackage.key);

const requireRegistryString = ({ field, moduleName, value }) => {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(
      moduleName
        ? `Module registry entry ${moduleName} ${field} is required`
        : `Module registry entry ${field} is required`
    );
  }
  return value.trim();
};

const normalizeRegistryConsolePackages = ({ consolePackages, moduleName }) => {
  if (consolePackages === undefined) {
    return [];
  }
  if (!Array.isArray(consolePackages)) {
    throw new TypeError(
      `Module registry entry ${moduleName} consolePackages must be an array`
    );
  }
  return consolePackages.map((consolePackage, index) => ({
    exportName: requireRegistryString({
      field: `consolePackages[${index}].exportName`,
      moduleName,
      value: consolePackage?.exportName,
    }),
    packageName: requireRegistryString({
      field: `consolePackages[${index}].packageName`,
      moduleName,
      value: consolePackage?.packageName,
    }),
    route:
      typeof consolePackage?.route === "string"
        ? consolePackage.route.trim()
        : "-",
  }));
};

const normalizeRegistryEntry = (entry) => {
  if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
    throw new Error("Module registry entries must be JSON objects");
  }
  const name = requireRegistryString({
    field: "name",
    value: entry.name,
  });
  const source = requireRegistryString({
    field: "source",
    moduleName: name,
    value: entry.source,
  });
  if (source !== "remote") {
    throw new Error(`Module registry entry ${name} source must be remote`);
  }
  const capabilities = entry.capabilities ?? [];
  if (!Array.isArray(capabilities)) {
    throw new TypeError(
      `Module registry entry ${name} capabilities must be an array`
    );
  }
  return {
    baseUrl:
      typeof entry.baseUrl === "string" && entry.baseUrl.trim()
        ? trimTrailingSlash(entry.baseUrl.trim())
        : undefined,
    capabilities: capabilities.map(String),
    consolePackages: normalizeRegistryConsolePackages({
      consolePackages: entry.consolePackages,
      moduleName: name,
    }),
    manifestReference: requireRegistryString({
      field: "manifestReference",
      moduleName: name,
      value: entry.manifestReference,
    }),
    name,
    source,
    summary:
      typeof entry.summary === "string" && entry.summary.trim()
        ? entry.summary.trim()
        : undefined,
    version: requireRegistryString({
      field: "version",
      moduleName: name,
      value: entry.version,
    }),
  };
};

const readModuleRegistry = async ({ options }) => {
  const repoRoot = options.repoRoot
    ? path.resolve(options.repoRoot)
    : await findRepoRoot(process.cwd());
  const registryFilePath = path.resolve(
    options.registryFile ?? path.join(repoRoot, ".lenso/module-registry.json")
  );
  const registry = await readJson(registryFilePath);
  if (!registry || typeof registry !== "object" || Array.isArray(registry)) {
    throw new Error("Module registry catalog must be a JSON object");
  }
  if (registry.version !== 1) {
    throw new Error("Module registry catalog version must be 1");
  }
  if (!Array.isArray(registry.modules)) {
    throw new TypeError("Module registry catalog modules must be an array");
  }
  const entries = registry.modules.map(normalizeRegistryEntry);
  const seenModuleNames = new Set();
  for (const entry of entries) {
    if (seenModuleNames.has(entry.name)) {
      throw new Error(`Duplicate module registry entry: ${entry.name}`);
    }
    seenModuleNames.add(entry.name);
  }
  return {
    entries,
    registryFilePath,
    repoRoot,
  };
};

const findRegistryModule = ({ entries, moduleName }) => {
  const entry = entries.find((candidate) => candidate.name === moduleName);
  if (!entry) {
    const available = entries.map((candidate) => candidate.name).join(", ");
    throw new Error(
      `Module registry does not contain module ${moduleName}${
        available ? `. Available modules: ${available}` : ""
      }`
    );
  }
  return entry;
};

const formatListValue = (items) => (items.length > 0 ? items.join(", ") : "-");

const listModuleRegistry = async ({ options }) => {
  const { entries, registryFilePath, repoRoot } = await readModuleRegistry({
    options,
  });
  console.log("Module registry entries:");
  console.log(`- catalog: ${path.relative(repoRoot, registryFilePath)}`);
  for (const entry of entries) {
    console.log(`- ${entry.name} ${entry.version} ${entry.source}`);
    if (entry.summary) {
      console.log(`  summary: ${entry.summary}`);
    }
    console.log(`  manifest: ${entry.manifestReference}`);
    if (entry.baseUrl) {
      console.log(`  base URL: ${entry.baseUrl}`);
    }
    console.log(`  capabilities: ${formatListValue(entry.capabilities)}`);
    console.log(
      `  console packages: ${formatListValue(
        consolePackagesFromRegistryEntry(entry)
      )}`
    );
  }
};

const inspectRegistryModule = async ({ moduleName, options }) => {
  const { entries } = await readModuleRegistry({ options });
  const entry = findRegistryModule({ entries, moduleName });
  const manifest = await readJsonFromReference(entry.manifestReference);
  const remoteModule = validateRemoteModuleManifest(manifest);
  if (remoteModule.name !== entry.name) {
    throw new Error(
      `Registry entry ${entry.name} points to manifest for ${remoteModule.name}`
    );
  }
  const baseUrl = deriveRemoteBaseUrl({
    baseUrl: entry.baseUrl ?? options.baseUrl,
    manifestReference: entry.manifestReference,
  });

  console.log(`Registry module ${entry.name}`);
  console.log(`- catalog version: ${entry.version}`);
  console.log(`- manifest version: ${remoteModule.version}`);
  console.log(`- source: ${entry.source}`);
  console.log(`- manifest: ${entry.manifestReference}`);
  console.log(`- base URL: ${baseUrl}`);
  console.log(`- manifest status: ok`);
  console.log(`- capabilities: ${formatListValue(manifest.capabilities)}`);
  console.log(
    `- console packages: ${formatListValue(consolePackagesFromManifest(manifest))}`
  );
};

const installRegistryModule = async ({ moduleName, options }) => {
  const { entries } = await readModuleRegistry({ options });
  const entry = findRegistryModule({ entries, moduleName });
  await inspectRegistryModule({ moduleName, options });
  await addRemoteModule({
    manifestReference: entry.manifestReference,
    options: {
      ...options,
      baseUrl: entry.baseUrl ?? options.baseUrl,
    },
  });
  console.log(`Installed registry module ${entry.name}.`);
};

const registryDoctorIssueGroups = [
  "Catalog",
  "Manifest",
  "Console package hint",
];

const formatRegistryDoctorIssues = (issues) => {
  const lines = [`Module registry doctor found ${issues.length} issue(s).`];
  for (const group of registryDoctorIssueGroups) {
    const groupIssues = issues.filter((issue) => issue.group === group);
    if (groupIssues.length === 0) {
      continue;
    }
    lines.push("", `${group}:`);
    for (const issue of groupIssues) {
      lines.push(`- ${issue.message}`);
      lines.push(`  fix: ${issue.fix}`);
    }
  }
  return lines.join("\n");
};

const addRegistryDoctorIssue = ({ fix, group, issues, message }) => {
  issues.push({ fix, group, message });
};

const compareRegistryConsolePackages = ({ entry, issues, manifest }) => {
  const manifestPackages = new Set(consolePackagesFromManifest(manifest));
  const entryPackages = consolePackagesFromRegistryEntry(entry);
  for (const packageKey of entryPackages) {
    if (!manifestPackages.has(packageKey)) {
      addRegistryDoctorIssue({
        fix: `sync ${entry.name} consolePackages with the remote manifest console declarations`,
        group: "Console package hint",
        issues,
        message: `${packageKey} is not declared by manifest ${manifest.name}`,
      });
    }
  }
};

const checkRegistryEntryManifest = async ({ entry, issues, options }) => {
  try {
    deriveRemoteBaseUrl({
      baseUrl: entry.baseUrl ?? options.baseUrl,
      manifestReference: entry.manifestReference,
    });
  } catch {
    addRegistryDoctorIssue({
      fix: "add baseUrl or use a manifest URL ending with /manifest",
      group: "Catalog",
      issues,
      message: `${entry.name} baseUrl is missing`,
    });
  }

  let manifest;
  try {
    manifest = await readJsonFromReference(entry.manifestReference);
  } catch (error) {
    addRegistryDoctorIssue({
      fix: `verify ${entry.name} manifestReference and network access`,
      group: "Manifest",
      issues,
      message: `${entry.name} manifest could not be read: ${error.message}`,
    });
    return { consolePackageHints: entry.consolePackages.length };
  }

  let remoteModule;
  try {
    remoteModule = validateRemoteModuleManifest(manifest);
  } catch (error) {
    addRegistryDoctorIssue({
      fix: `update ${entry.name} manifest so it is a valid remote module manifest`,
      group: "Manifest",
      issues,
      message: `${entry.name} manifest is invalid: ${error.message}`,
    });
    return { consolePackageHints: entry.consolePackages.length };
  }

  if (remoteModule.name !== entry.name) {
    addRegistryDoctorIssue({
      fix: `update catalog name to ${remoteModule.name} or point ${entry.name} at the correct manifest`,
      group: "Manifest",
      issues,
      message: `${entry.name} catalog name does not match manifest name ${remoteModule.name}`,
    });
  }
  if (remoteModule.version !== entry.version) {
    addRegistryDoctorIssue({
      fix: `update ${entry.name} catalog version to ${remoteModule.version}`,
      group: "Manifest",
      issues,
      message: `${entry.name} catalog version ${entry.version} does not match manifest version ${remoteModule.version}`,
    });
  }
  compareRegistryConsolePackages({ entry, issues, manifest });
  return { consolePackageHints: entry.consolePackages.length };
};

const runModuleRegistryDoctor = async ({ options }) => {
  const { entries } = await readModuleRegistry({ options });
  const issues = [];
  let consolePackageHints = 0;
  for (const entry of entries) {
    const result = await checkRegistryEntryManifest({ entry, issues, options });
    consolePackageHints += result.consolePackageHints;
  }

  if (issues.length > 0) {
    throw new Error(formatRegistryDoctorIssues(issues));
  }

  console.log("Module registry doctor passed.");
  console.log(`- catalog modules: ${entries.length}`);
  console.log(`- console package hints: ${consolePackageHints}`);
};

const doctorIssueGroups = [
  "Remote source",
  "Console package",
  "Registry mapping",
];

const addDoctorIssue = ({ fix, group, issues, message }) => {
  issues.push({ fix, group, message });
};

const formatDoctorIssues = (issues) => {
  const lines = [`Module doctor found ${issues.length} issue(s).`];
  for (const group of doctorIssueGroups) {
    const groupIssues = issues.filter((issue) => issue.group === group);
    if (groupIssues.length === 0) {
      continue;
    }
    lines.push("", `${group}:`);
    for (const issue of groupIssues) {
      lines.push(`- ${issue.message}`);
      lines.push(`  fix: ${issue.fix}`);
    }
  }
  return lines.join("\n");
};

const runModuleDoctor = async ({ options }) => {
  const repoRoot = options.repoRoot
    ? path.resolve(options.repoRoot)
    : await findRepoRoot(process.cwd());
  const runtimeConsoleRoot = path.resolve(
    options.runtimeConsoleRoot ?? path.join(repoRoot, "apps/runtime-console")
  );
  const envFilePath = path.resolve(
    options.envFile ?? path.join(repoRoot, ".env")
  );
  const installPlanPath = path.resolve(
    options.installPlanFile ??
      path.join(repoRoot, ".lenso/console-package-install-plan.json")
  );
  const paths = runtimeConsolePaths(runtimeConsoleRoot);
  const remoteModules = await remoteModulesFromEnvFile(envFilePath);
  const remoteModulesByName = new Map(
    remoteModules.map((remoteModule) => [remoteModule.name, remoteModule])
  );
  const installPlan = await readJson(installPlanPath);
  const packageJson = await readJson(paths.packageJsonPath);
  const manifestExportsSource = await readFile(
    paths.manifestExportsPath,
    "utf-8"
  );
  const moduleExportsSource = await readFile(paths.moduleExportsPath, "utf-8");
  const issues = [];

  for (const modulePlan of installPlan.modules ?? []) {
    const { moduleName } = modulePlan;
    const remoteModule = remoteModulesByName.get(moduleName);
    if (!remoteModule) {
      addDoctorIssue({
        fix: `lenso module add <manifest-url> --base-url ${modulePlan.baseUrl ?? "<base-url>"}`,
        group: "Remote source",
        issues,
        message: `REMOTE_MODULES is missing module ${moduleName}`,
      });
    } else if (
      modulePlan.baseUrl &&
      remoteModule.baseUrl !== modulePlan.baseUrl
    ) {
      addDoctorIssue({
        fix: `lenso module add <manifest-url> --base-url ${modulePlan.baseUrl}`,
        group: "Remote source",
        issues,
        message: `REMOTE_MODULES base URL for ${moduleName} is ${remoteModule.baseUrl}, expected ${modulePlan.baseUrl}`,
      });
    }

    for (const consolePackage of modulePlan.consolePackages ?? []) {
      const manifestName = manifestNameFromModuleExport(
        consolePackage.exportName
      );
      const { packageName } = consolePackage;
      if (!packageJson.dependencies?.[packageName]) {
        addDoctorIssue({
          fix: `pnpm --dir apps/runtime-console add ${packageName}`,
          group: "Console package",
          issues,
          message: `Runtime Console dependency is missing: ${packageName}`,
        });
      }
      if (!manifestExportsSource.includes(packageName)) {
        addDoctorIssue({
          fix: `lenso console-package apply-plan --repo-root ${repoRoot}`,
          group: "Registry mapping",
          issues,
          message: `Console package manifest import is missing: ${packageName}`,
        });
      }
      if (!manifestExportsSource.includes(`${manifestName},`)) {
        addDoctorIssue({
          fix: `lenso console-package apply-plan --repo-root ${repoRoot}`,
          group: "Registry mapping",
          issues,
          message: `Console package manifest export is missing: ${manifestName}`,
        });
      }
      if (!moduleExportsSource.includes(packageName)) {
        addDoctorIssue({
          fix: `lenso console-package apply-plan --repo-root ${repoRoot}`,
          group: "Registry mapping",
          issues,
          message: `Console package module import is missing: ${packageName}`,
        });
      }
      if (
        !moduleExportsSource.includes(
          `[consolePackageKey(${manifestName})]: ${consolePackage.exportName}`
        )
      ) {
        addDoctorIssue({
          fix: `lenso console-package apply-plan --repo-root ${repoRoot}`,
          group: "Registry mapping",
          issues,
          message: `Console package module mapping is missing: ${consolePackageKey(
            {
              exportName: consolePackage.exportName,
              packageName,
            }
          )}`,
        });
      }
    }
  }

  if (issues.length > 0) {
    throw new Error(formatDoctorIssues(issues));
  }

  console.log("Module doctor passed.");
  console.log(`- remote modules: ${remoteModules.length}`);
  console.log(
    `- console package plan items: ${uniqueConsolePackagePlanItems(installPlan).length}`
  );
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

const addModuleDoctorOptions = (command) =>
  command
    .option("--repo-root <path>", "Lenso host repository root")
    .option("--runtime-console-root <path>", "Runtime Console app root")
    .option("--env-file <path>", "env file to inspect")
    .option("--install-plan-file <path>", "console package install plan file");

const addModuleRegistryOptions = (command) =>
  command
    .option("--repo-root <path>", "Lenso host repository root")
    .option("--registry-file <path>", "module registry catalog file");

const addModuleRegistryInstallOptions = (command) =>
  addModuleRegistryOptions(command)
    .option("--env-file <path>", "env file to update")
    .option("--install-plan-file <path>", "console package install plan file")
    .option(
      "--base-url <url>",
      "override the remote module base URL from the registry"
    )
    .option("--dry-run", "print install changes without writing them");

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
Third-party remote module flow:
  lenso module registry list
  lenso module registry doctor
  lenso module registry inspect <module>
  lenso module registry install <module>
  lenso module add <manifest-url>
  lenso console-package apply-plan
  lenso module doctor
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
  addModuleDoctorOptions(
    moduleCommand
      .command("doctor")
      .description("check configured remote modules and console packages")
  ).action(async (options) => {
    await runModuleDoctor({ options });
  });

  const registryCommand = moduleCommand
    .command("registry")
    .description("discover and install remote modules from a catalog");
  addModuleRegistryOptions(
    registryCommand.command("list").description("list registry modules")
  ).action(async (options) => {
    await listModuleRegistry({ options });
  });
  addModuleRegistryOptions(
    registryCommand
      .command("doctor")
      .description("check every registry module manifest before installation")
  ).action(async (options) => {
    await runModuleRegistryDoctor({ options });
  });
  addModuleRegistryOptions(
    registryCommand
      .command("inspect <moduleName>")
      .description("inspect a registry module and validate its manifest")
  ).action(async (moduleName, options) => {
    await inspectRegistryModule({ moduleName, options });
  });
  addModuleRegistryInstallOptions(
    registryCommand
      .command("install <moduleName>")
      .description("install a registry module through the remote install flow")
  ).action(async (moduleName, options) => {
    await installRegistryModule({ moduleName, options });
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
