import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";
import {
  Executable,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Trace,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

function configuredServerPath(context: vscode.ExtensionContext): string {
  const configuration = vscode.workspace.getConfiguration("lab.languageServer");
  const configured = configuration.get<string>("path", "").trim();
  if (configured) {
    return configured.startsWith("~/")
      ? path.join(os.homedir(), configured.slice(2))
      : configured;
  }

  const executableName =
    process.platform === "win32"
      ? "lab-language-server.exe"
      : "lab-language-server";
  const bundled = context.asAbsolutePath(path.join("server", executableName));
  if (fs.existsSync(bundled)) {
    if (process.platform !== "win32") {
      try {
        fs.accessSync(bundled, fs.constants.X_OK);
      } catch {
        fs.chmodSync(bundled, 0o755);
      }
    }
    return bundled;
  }
  return executableName;
}

function createClient(context: vscode.ExtensionContext): LanguageClient {
  const command = configuredServerPath(context);
  const executable: Executable = { command };
  const serverOptions: ServerOptions = {
    run: executable,
    debug: executable,
  };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "lab" }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.{lab,toml}"),
    },
    outputChannelName: "Lab Language Server",
  };
  return new LanguageClient(
    "labLanguageServer",
    "Lab Language Server",
    serverOptions,
    clientOptions,
  );
}

async function startClient(context: vscode.ExtensionContext): Promise<void> {
  const nextClient = createClient(context);
  client = nextClient;
  try {
    await nextClient.start();
    const trace = vscode.workspace
      .getConfiguration("lab.languageServer")
      .get<string>("trace", "off");
    await nextClient.setTrace(Trace.fromString(trace));
  } catch (error) {
    client = undefined;
    throw error;
  }
}

async function restartClient(context: vscode.ExtensionContext): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
  await startClient(context);
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
    vscode.commands.registerCommand("lab.restartLanguageServer", () =>
      restartClient(context),
    ),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("lab.languageServer.path")) {
        void restartClient(context);
      } else if (event.affectsConfiguration("lab.languageServer.trace") && client) {
        const trace = vscode.workspace
          .getConfiguration("lab.languageServer")
          .get<string>("trace", "off");
        void client.setTrace(Trace.fromString(trace));
      }
    }),
  );
  await startClient(context);
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
}
