# 0008: Editor architecture and crate workspace

Status: accepted for the current milestone

## Decision

All Rust packages live under `crates/`, named for the responsibility they own.
Binary names remain independent from package names:

| Package | Responsibility | Binary or host surface |
| --- | --- | --- |
| `lab-cli` | project and real-world workflow UX | `lab` |
| `lab-compiler` | lowering, backend IR, passes, compiler inspection | `labc`, `lab-opt` |
| `lab-language` | syntax, AST, type checking, action contracts, material-flow checking, source diagnostics | Rust API |
| `lab-package` | manifests, source discovery, module graph | Rust API |
| `lab-ide` | document snapshots and editor intelligence | Rust API |
| `lab-language-server` | JSON-RPC/LSP transport and UTF-16 conversion | `lab-language-server` |
| `lab-ide-wasm` | browser and embedded bindings | WebAssembly API |

The `lab-language-server` executable is editor infrastructure. It is not a
second user-facing command suite and it does not absorb project or workflow
commands from `lab`.

`lab-ide` is deliberately separate from the language server. It owns semantic
editor operations over in-memory source IDs and byte spans without filesystem,
process, JSON-RPC, or LSP types. Native editors use it through
`lab-language-server`; browser editors and desktop embeddings use it through
`lab-ide-wasm` or the Rust API directly.

The VS Code/Cursor extension remains thin. TextMate handles initial colorization
and the extension launches the server for diagnostics, completion, hover,
navigation, references, rename, document symbols, semantic tokens, and
formatting.

## Source and diagnostic contract

`lab-language` returns an `Analysis` rather than forcing editor hosts through a
fail-fast compiler result. An analysis includes:

- an opaque host-selected source ID;
- the parsed AST when parsing succeeded, even when later checking failed;
- the checked portable module when all frontend verification succeeded;
- a list of diagnostics with stable codes, severities, source spans, and room
  for related locations.

Core spans are half-open UTF-8 byte ranges. The LSP adapter alone converts them
to and from UTF-16 line/column positions. This keeps compiler, native embedded,
and WebAssembly consumers independent of one editor protocol.

## Package boundary

`lab-package` owns filesystem discovery and a deterministic module graph. The
graph can distinguish standard-library, same-package, and declared-dependency
imports without fetching or silently accepting dependencies. The CLI continues
to fail closed on declared dependencies until dependency acquisition, locking,
and imported public-symbol checking are implemented together.

## Deliberately unfinished

This milestone establishes stable seams, not a claim of mature IDE semantics.
The parser currently emits one syntax diagnostic per analysis. Name-based
navigation handles open documents and top-level declarations; lexical scopes,
imported public symbols, and symbol identities need to replace textual fallback
matching. Document updates reanalyze one whole module rather than maintaining an
incremental syntax tree. The formatter is intentionally conservative and only
normalizes trailing whitespace and the final newline.

Those upgrades belong inside `lab-language` and `lab-ide`; they should not leak
compiler logic into an editor extension or protocol types into the language.
