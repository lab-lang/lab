import * as vscode from "vscode";
import {
  Executable,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

function createClient(): LanguageClient {
  const configuration = vscode.workspace.getConfiguration("lab.languageServer");
  const command = configuration.get<string>("path", "lab-language-server");
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

async function restartClient(): Promise<void> {
  if (client) {
    await client.stop();
  }
  client = createClient();
  await client.start();
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  context.subscriptions.push(
    vscode.commands.registerCommand("lab.restartLanguageServer", restartClient),
  );
  client = createClient();
  await client.start();
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
}
