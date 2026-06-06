import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import path from "node:path";

const runtimeConsoleRoot = path.resolve(import.meta.dirname, "..");
const packageJsonPath = path.join(runtimeConsoleRoot, "package.json");
const tsconfigPath = path.join(runtimeConsoleRoot, "tsconfig.json");
const viteConfigPath = path.join(runtimeConsoleRoot, "vite.config.ts");
const oxlintConfigPath = path.join(runtimeConsoleRoot, "oxlint.config.ts");
const manifestExportsPath = path.join(
  runtimeConsoleRoot,
  "src/console-package-manifest-exports.ts"
);
const moduleExportsPath = path.join(
  runtimeConsoleRoot,
  "src/console-package-module-exports.ts"
);

const readJson = async (filePath) =>
  JSON.parse(await readFile(filePath, "utf-8"));

const relativePath = (filePath) => path.relative(runtimeConsoleRoot, filePath);

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

const pascalCase = (value) => {
  const camel = camelCase(value);
  return `${camel.charAt(0).toUpperCase()}${camel.slice(1)}`;
};

const exportStemFromPackageSlug = (packageSlugValue) => {
  const normalized = packageSlugValue.replace(/-console$/u, "");
  return `${camelCase(normalized)}Console`;
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

const printUsage = () => {
  console.log(`Usage:
  pnpm create:console-package <module-id> [options]

Options:
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

const updatePackageJson = async ({
  packageName,
  packageSlug,
  pendingWrites,
}) => {
  const packageJson = await readJson(packageJsonPath);
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
    packageJsonPath,
    `${JSON.stringify(packageJson, null, 2)}\n`
  );
};

const updateTsconfig = async ({ packageName, packageSlug, pendingWrites }) => {
  const tsconfig = await readJson(tsconfigPath);
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
    tsconfigPath,
    `${JSON.stringify(tsconfig, null, 2)}\n`
  );
};

const updateViteConfig = async ({
  packageName,
  packageSlug,
  pendingWrites,
}) => {
  const fileSource = await readFile(viteConfigPath, "utf-8");
  const entry = `      "${packageName}": fileURLToPath(
        new URL("packages/${packageSlug}/src/index.tsx", import.meta.url)
      ),
`;
  queueWrite(
    pendingWrites,
    viteConfigPath,
    insertBeforeNeedle(fileSource, entry, '      "@lenso/runtime-console-api":')
  );
};

const updateOxlintConfig = async ({ packageSlug, pendingWrites }) => {
  const fileSource = await readFile(oxlintConfigPath, "utf-8");
  const entry = `        "packages/${packageSlug}/src/**/*.{ts,tsx}",
`;
  queueWrite(
    pendingWrites,
    oxlintConfigPath,
    insertBeforeNeedle(fileSource, entry, '        "vite.config.ts",')
  );
};

const updateManifestExports = async ({
  manifestName,
  packageName,
  pendingWrites,
}) => {
  let fileSource = await readFile(manifestExportsPath, "utf-8");
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
  queueWrite(pendingWrites, manifestExportsPath, fileSource);
};

const updateModuleExports = async ({
  manifestName,
  moduleName,
  packageName,
  pendingWrites,
}) => {
  let fileSource = await readFile(moduleExportsPath, "utf-8");
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
  queueWrite(pendingWrites, moduleExportsPath, fileSource);
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
    path.join(packageDir, "src/manifest.ts"),
    `import { defineConsolePackageManifest } from "@lenso/runtime-console-api";

export const ${manifestName} = defineConsolePackageManifest({
  area: "${area}",
  exportName: "${moduleName}",
  icon: "${icon}",
  id: "${moduleId}",
  label: "${label}",
  packageName: "${packageName}",
  requiredCapabilities: ["${capability}"],
  route: "${route}",
  source: "${registrySource}",
  surfaceName: "${surfaceName}",
  version: "workspace",
} as const);
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

const main = async () => {
  const options = parseOptions(process.argv.slice(2));

  if (options.help || !options.moduleId) {
    printUsage();
    process.exit(options.help ? 0 : 1);
  }

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
  const { dryRun } = options;

  if (await pathExists(packageDir)) {
    throw new Error(
      `Console package directory already exists: ${relativePath(packageDir)}`
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

  if (dryRun) {
    console.log("Console package dry run:");
    for (const filePath of pendingWrites.keys()) {
      console.log(`- ${relativePath(filePath)}`);
    }
    return;
  }

  await writePendingFiles(pendingWrites);

  console.log(`Created ${packageName}.`);
  console.log("Next steps:");
  console.log("- pnpm install --lockfile-only");
  console.log("- pnpm check:console-packages");
  console.log("- just console-check");
};

await main();
