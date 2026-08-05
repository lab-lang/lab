# `lab` CLI

`lab` is the command line for Lab. It manages projects and packages today and will grow into the operational interface for live laboratory runs.

Install the current CLI from the repository root:

```sh
cargo install --path crates/lab-cli --locked
```

This installs `lab` into Cargo's binary directory, normally `~/.cargo/bin`. Reinstall after local CLI changes with `cargo install --path crates/lab-cli --locked --force`.

The initial project loop is:

```sh
lab new tet-reporter
cd tet-reporter
lab check
lab build
```

`lab.toml` anchors package identity and the build entry. Source modules are discovered recursively under `src/` and receive stable names from their package and relative path. Same-package imports and recursive path dependencies are compiled through checked module interfaces. `lab build` writes verified portable module IR plus a package index under `.lab/build/` and a deterministic `lab.lock` next to the manifest.

All read-oriented commands support `--json` for editor and automation clients:

```sh
lab metadata --json
lab check --json
```

Path dependencies may optionally carry a semver requirement, which is checked against the dependency manifest. Registry dependencies remain explicitly unsupported and fail closed; adding them requires a registry protocol and integrity model rather than silent fallback. Workflow execution commands will be added only when the durable runtime has real run semantics.
