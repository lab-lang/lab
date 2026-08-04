# Lab for VS Code and Cursor

Language support for `.lab` files:

- syntax highlighting for biological declarations, workflow control, typed values, quantities, and durable actions;
- indentation, folding, comments, brackets, and editor word boundaries;
- starter snippets for workflows, reactive handlers, and outcomes;
- source-aware diagnostics, completion, hover, navigation, references, rename, document symbols, semantic highlighting, and formatting through `lab-language-server`.

## Install in Cursor

Build a platform-specific VSIX from the repository root:

```sh
cd editors/vscode
npm ci
npm run install:cursor
```

This builds a platform-specific VSIX and installs it through Cursor's command-line interface. The package contains a release build of `lab-language-server` for the machine that built it, so no separate server installation or Cursor setting is needed. Restart Cursor after first installation if an already-open `.lab` editor does not activate the extension.

Run `npm run package` instead when you only want to create the VSIX. Its exact path is printed when packaging completes.

To update a local installation after changing the server or extension, rerun `npm run package` and install the newly generated VSIX with `--force`.

## Development

For TypeScript-only development, run `npm ci` and `npm run compile`, then use the editor's Extension Development Host. The extension first uses `lab.languageServer.path` when configured, then a bundled server, then `lab-language-server` on `PATH`. This makes it possible to point the development host at a debug binary such as:

```json
{
  "lab.languageServer.path": "/absolute/path/to/lab/target/debug/lab-language-server"
}
```

Use **Lab: Restart Language Server** after rebuilding an explicitly configured server. Set `lab.languageServer.trace` to `messages` or `verbose` to inspect protocol traffic in the Lab language-server output channels.

The current editor engine analyzes complete in-memory documents. Navigation and rename are name-based across open `.lab` files; imported symbols, lexical identity, incremental parsing, multi-error parser recovery, and syntax-aware formatting remain future work.
