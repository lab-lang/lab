import { spawnSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  readFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const extensionRoot = resolve(scriptDirectory, "..");
const repositoryRoot = resolve(extensionRoot, "../..");
const executableName =
  process.platform === "win32"
    ? "lab-language-server.exe"
    : "lab-language-server";
const target = packageTarget(process.platform, process.arch);

run("cargo", [
  "build",
  "--release",
  "--locked",
  "-p",
  "lab-language-server",
], repositoryRoot);

const serverDirectory = join(extensionRoot, "server");
mkdirSync(serverDirectory, { recursive: true });
const bundledServer = join(serverDirectory, executableName);
copyFileSync(
  join(repositoryRoot, "target", "release", executableName),
  bundledServer,
);
if (process.platform !== "win32") {
  chmodSync(bundledServer, 0o755);
}
copyFileSync(join(repositoryRoot, "LICENSE"), join(extensionRoot, "LICENSE"));

const manifest = JSON.parse(
  readFileSync(join(extensionRoot, "package.json"), "utf8"),
);
const outputDirectory = join(extensionRoot, "dist");
mkdirSync(outputDirectory, { recursive: true });
const output = join(
  outputDirectory,
  `${manifest.name}-${manifest.version}-${target}.vsix`,
);
run(
  process.execPath,
  [
    join(extensionRoot, "node_modules", "@vscode", "vsce", "vsce"),
    "package",
    "--target",
    target,
    "--out",
    output,
  ],
  extensionRoot,
);
if (process.argv.includes("--install-cursor")) {
  run("cursor", ["--install-extension", output, "--force"], extensionRoot);
}
console.log(output);

function packageTarget(platform, architecture) {
  const architectures = {
    arm: "armhf",
    arm64: "arm64",
    ia32: "ia32",
    x64: "x64",
  };
  const packagedArchitecture = architectures[architecture];
  if (!packagedArchitecture) {
    throw new Error(`unsupported extension architecture: ${architecture}`);
  }
  if (platform === "darwin") {
    if (architecture !== "arm64" && architecture !== "x64") {
      throw new Error(`unsupported macOS extension architecture: ${architecture}`);
    }
    return `darwin-${packagedArchitecture}`;
  }
  if (platform === "linux") {
    return `linux-${packagedArchitecture}`;
  }
  if (platform === "win32") {
    return `win32-${packagedArchitecture}`;
  }
  throw new Error(`unsupported extension platform: ${platform}`);
}

function run(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    env: process.env,
    stdio: "inherit",
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`${command} exited with status ${result.status}`);
  }
}
