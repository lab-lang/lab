# Lab for VS Code and Cursor

Language support for `.lab` files:

- syntax highlighting for biological declarations, workflow control, typed
  values, quantities, and durable actions;
- indentation, folding, comments, brackets, and editor word boundaries;
- starter snippets for workflows, reactive handlers, and outcomes.
- source-aware diagnostics, completion, hover, navigation, references, rename,
  document symbols, semantic highlighting, and formatting through
  `lab-language-server`.

Cursor supports VS Code extensions, so the same package works in both editors.
During development, build `lab-language-server`, run `npm install` and
`npm run compile` in this directory, then use the editor's Extension Development
Host. Set `lab.languageServer.path` when the server is not on `PATH`.
