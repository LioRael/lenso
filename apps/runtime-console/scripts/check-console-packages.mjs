import { readFile } from "node:fs/promises";
import path from "node:path";

const readJson = async (filePath) =>
  JSON.parse(await readFile(filePath, "utf-8"));

const exportStemFromPackageSlug = (packageSlug) => {
  const normalized = packageSlug.replace(/-console$/u, "");
  return `${normalized.replaceAll(/-([a-z])/gu, (_match, letter) =>
    letter.toUpperCase()
  )}Console`;
};

const relativePath = (filePath) => path.relative(runtimeConsoleRoot, filePath);

const assertContains = ({ content, filePath, message, needle }) => {
  if (!content.includes(needle)) {
    errors.push(`${relativePath(filePath)} must ${message}`);
  }
};

const runtimeConsoleRoot = path.resolve(import.meta.dirname, "..");

const hostPackageJsonPath = path.join(runtimeConsoleRoot, "package.json");
const manifestExportsPath = path.join(
  runtimeConsoleRoot,
  "src/console-package-manifest-exports.ts"
);
const moduleExportsPath = path.join(
  runtimeConsoleRoot,
  "src/console-package-module-exports.ts"
);
const packageInstallManifestsPath = path.join(
  runtimeConsoleRoot,
  "src/console-package-install-manifests.ts"
);

const hostPackageJson = await readJson(hostPackageJsonPath);
const manifestExportsSource = await readFile(manifestExportsPath, "utf-8");
const moduleExportsSource = await readFile(moduleExportsPath, "utf-8");
const packageInstallManifestsSource = await readFile(
  packageInstallManifestsPath,
  "utf-8"
);

const workspacePackageNames = Object.keys(hostPackageJson.dependencies ?? {})
  .filter(
    (name) =>
      name.startsWith("@lenso/") && name !== "@lenso/runtime-console-api"
  )
  .toSorted();

const errors = [];

for (const packageName of workspacePackageNames) {
  const packageSlug = packageName.replace("@lenso/", "");
  const exportStem = exportStemFromPackageSlug(packageSlug);
  const manifestName = `${exportStem}Manifest`;
  const moduleName = `${exportStem}Module`;
  const packageJsonPath = path.join(
    runtimeConsoleRoot,
    "packages",
    packageSlug,
    "package.json"
  );
  const packageJson = await readJson(packageJsonPath).catch((error) => {
    errors.push(
      `Console package ${packageName} is declared as a host dependency, but ${relativePath(
        packageJsonPath
      )} could not be read: ${error.message}`
    );
    return null;
  });

  if (!packageJson) {
    continue;
  }

  if (packageJson.name !== packageName) {
    errors.push(
      `${relativePath(packageJsonPath)} name must be ${packageName}, got ${packageJson.name}`
    );
  }

  if (!packageJson.peerDependencies?.["@lenso/runtime-console-api"]) {
    errors.push(
      `${relativePath(packageJsonPath)} must declare @lenso/runtime-console-api as a peer dependency`
    );
  }

  assertContains({
    content: manifestExportsSource,
    filePath: manifestExportsPath,
    message: `import ${manifestName} from ${packageName}`,
    needle: manifestName,
  });
  assertContains({
    content: manifestExportsSource,
    filePath: manifestExportsPath,
    message: `import ${packageName}`,
    needle: packageName,
  });
  assertContains({
    content: manifestExportsSource,
    filePath: manifestExportsPath,
    message: `include ${manifestName} in consolePackageManifests`,
    needle: `${manifestName},`,
  });

  assertContains({
    content: moduleExportsSource,
    filePath: moduleExportsPath,
    message: `import ${manifestName} from ${packageName}`,
    needle: manifestName,
  });
  assertContains({
    content: moduleExportsSource,
    filePath: moduleExportsPath,
    message: `import ${moduleName} from ${packageName}`,
    needle: moduleName,
  });
  assertContains({
    content: moduleExportsSource,
    filePath: moduleExportsPath,
    message: `map ${manifestName} to ${moduleName}`,
    needle: `[consolePackageKey(${manifestName})]: ${moduleName}`,
  });
}

assertContains({
  content: packageInstallManifestsSource,
  filePath: packageInstallManifestsPath,
  message: "derive install manifests from consolePackageManifests",
  needle: "consolePackageManifests.map",
});

if (errors.length > 0) {
  console.error("Console package registration check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log(
  `Console package registration check passed for ${workspacePackageNames.length} package(s).`
);
