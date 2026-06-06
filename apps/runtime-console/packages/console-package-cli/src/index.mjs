#!/usr/bin/env node
import { realpathSync } from "node:fs";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";

const readJson = async (filePath) =>
  JSON.parse(await readFile(filePath, "utf-8"));

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

const printUsage = () => {
  console.log(`Usage:
  lenso module create <module-id> [options]
  lenso console-package create <module-id> [options]
  lenso-console-package create <module-id> [options]

Options:
  --repo-root <path>
  --runtime-console-root <path>
  --area <data|runtime|operations|configuration>
  --label <label>
  --route <route>
  --capability <capability>
  --icon <icon>
  --source <installed|first_party>
  --package-slug <name-console>
  --surface-name <name>
  --dry-run`);
};

const parseOptions = (args) => {
  const parsed = {
    dryRun: false,
    help: false,
  };
  const positional = [];

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--") {
      continue;
    }
    if (arg === "--dry-run") {
      parsed.dryRun = true;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      parsed.help = true;
      continue;
    }
    if (arg.startsWith("--")) {
      const key = camelCase(arg.slice(2));
      const value = args[index + 1];
      if (!value || value.startsWith("--")) {
        throw new Error(`${arg} requires a value`);
      }
      parsed[key] = value;
      index += 1;
      continue;
    }
    positional.push(arg);
  }

  const [moduleId] = positional;
  parsed.moduleId = moduleId;
  return parsed;
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
  pendingWrites,
  route,
  registrySource,
  surfaceName,
}) => {
  const consoleSurfaceContract = {
    area,
    exportName: moduleName,
    icon,
    id: moduleId,
    label,
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
          "@lenso/runtime-console-api": "workspace:*",
          react: "^19.1.0",
        },
        private: true,
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

const moduleManifest = ({ moduleId }) => `use platform_core::AppContext;
use platform_module::{LinkedBinding, Module, ModuleManifest};

/// Context-free manifest: serializable metadata only.
pub fn manifest() -> ModuleManifest {
    ModuleManifest::builder("${moduleId}").build()
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

const queueModuleFiles = ({ moduleDir, moduleId, pendingWrites }) => {
  queueWrite(
    pendingWrites,
    path.join(moduleDir, "Cargo.toml"),
    moduleCargoToml({ moduleId })
  );
  queueWrite(pendingWrites, path.join(moduleDir, "src/lib.rs"), moduleLib());
  queueWrite(
    pendingWrites,
    path.join(moduleDir, "src/module.rs"),
    moduleManifest({ moduleId })
  );
};

const createModule = async ({ options }) => {
  const repoRoot = options.repoRoot
    ? path.resolve(options.repoRoot)
    : await findRepoRoot(process.cwd());
  const moduleId = slugify(options.moduleId);
  if (!moduleId) {
    throw new Error("Module id is required");
  }
  const moduleCrate = snakeCase(moduleId);
  const moduleDir = path.join(repoRoot, "modules", moduleId);

  if (await pathExists(moduleDir)) {
    throw new Error(`Module directory already exists: modules/${moduleId}`);
  }

  const paths = repoPaths(repoRoot);
  const pendingWrites = new Map();
  const moduleContext = {
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
    return;
  }

  await writePendingFiles(pendingWrites);

  console.log(`Created module ${moduleId}.`);
  console.log("Next steps:");
  console.log(`- cargo test --locked -p ${moduleCrate}`);
  console.log("- just rust-check");
  console.log("- just arch-check");
};

const createConsolePackage = async ({ defaultRuntimeConsoleRoot, options }) => {
  const runtimeConsoleRoot = path.resolve(
    options.runtimeConsoleRoot ?? defaultRuntimeConsoleRoot ?? process.cwd()
  );
  const paths = runtimeConsolePaths(runtimeConsoleRoot);
  const moduleId = slugify(options.moduleId);
  const packageSlug = slugify(options.packageSlug ?? `${moduleId}-console`);
  const packageName = `@lenso/${packageSlug}`;
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

  if (await pathExists(packageDir)) {
    throw new Error(
      `Console package directory already exists: ${relativePath(
        runtimeConsoleRoot,
        packageDir
      )}`
    );
  }

  const pendingWrites = new Map();
  const packageContext = {
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
    packageSlug,
    paths,
    pendingWrites,
    registrySource,
    route,
    surfaceName,
  };

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

  console.log(`Created ${packageName}.`);
  console.log("Next steps:");
  console.log(
    `- Copy ${packageSlug}/console-surface.rs into the Rust module manifest`
  );
  console.log("- pnpm install --lockfile-only");
  console.log("- pnpm check:console-packages");
  console.log("- just console-check");
};

export const runConsolePackageCli = async (
  args,
  { defaultRuntimeConsoleRoot } = {}
) => {
  const [command, subcommand, ...rest] = args;
  if (command === "--help" || command === "-h" || !command) {
    printUsage();
    return command ? 0 : 1;
  }
  if (command === "module") {
    if (subcommand !== "create") {
      console.error(`Unknown module command: ${subcommand ?? ""}`.trim());
      printUsage();
      return 1;
    }
    const options = parseOptions(rest);
    if (options.help || !options.moduleId) {
      printUsage();
      return options.help ? 0 : 1;
    }
    await createModule({ options });
    return 0;
  }

  const isConsolePackageCreate =
    command === "create" ||
    (command === "console-package" && subcommand === "create");
  if (!isConsolePackageCreate) {
    console.error(`Unknown command: ${command}`);
    printUsage();
    return 1;
  }

  const consolePackageArgs =
    command === "create"
      ? [subcommand, ...rest].filter((arg) => arg !== undefined)
      : rest;
  const options = parseOptions(consolePackageArgs);
  if (options.help || !options.moduleId) {
    printUsage();
    return options.help ? 0 : 1;
  }

  await createConsolePackage({ defaultRuntimeConsoleRoot, options });
  return 0;
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
