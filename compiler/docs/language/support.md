# Language support

Support is tracked by compiler phase. `Lower` means verified portable module IR
unless the row explicitly names the older executable artifact pipeline. It does
not imply target selection or physical execution.

| Feature | Parse | Resolve | Type | Lower | Execute |
| --- | --- | --- | --- | --- | --- |
| Indented plasmid declaration | yes | yes | yes | yes | yes |
| Pure binding with `=` | plasmid fields | plasmid fields | plasmid fields | yes | yes |
| Quantity literals | acceptance subset | built-in units | dimension subset | yes | yes |
| `require` predicates | topology subset | yes | yes | yes | yes |
| `accept` predicates | sequence/concentration/volume | yes | yes | yes | yes |
| Built-in `std` module imports | yes | yes | n/a | yes | no |
| Project/package imports | yes | no | no | no | no |
| Circuit declarations and applications | yes | yes | yes | yes | no |
| Top-level pure bindings | yes | yes | yes | yes | no |
| `record`, `material`, `observation`, `evidence`, and `event` | yes | yes | yes | yes | no |
| Biological `part` declarations | syntax pending | no | no | no | no |
| Tagged `outcome` declarations and constructors | yes | yes | yes | yes | no |
| Workflow declarations and calls | yes | yes | yes | yes | no |
| Pure workflow bindings | yes | yes | yes | yes | no |
| Built-in durable operations with `<-` | yes | yes | yes | yes | no |
| `return` and output checking | yes | yes | yes | yes | no |
| `match` / `case` with continuing-branch bindings | yes | yes | yes | yes | no |
| `if` / `else` and `for` / `in` | yes | yes | yes | yes | no |
| `when every` / `when after` | yes | yes | yes | yes | no |
| Event emission | yes | yes | yes | yes | no |
| Affine material-flow checking in portable workflows | n/a | n/a | material kinds only | no | no |
| Durable workflow runtime | no | no | no | no | no |

The older executable artifact pipeline deliberately requires one directly
specified plasmid. Complete modules use the separate portable-module boundary;
requesting artifact-specific target IR, plans, or simulation for them still
reports an explicit unsupported-feature diagnostic.
