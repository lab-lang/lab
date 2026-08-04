# Language support

Support is tracked by compiler phase. Parsing a construct does not imply that
the compiler can lower or execute it.

| Feature | Parse | Resolve | Type | Lower | Execute |
| --- | --- | --- | --- | --- | --- |
| Indented plasmid declaration | yes | yes | yes | yes | yes |
| Pure binding with `=` | plasmid fields | plasmid fields | plasmid fields | yes | yes |
| Quantity literals | acceptance subset | built-in units | dimension subset | yes | yes |
| `require` predicates | topology subset | yes | yes | yes | yes |
| `accept` predicates | sequence/concentration/volume | yes | yes | yes | yes |
| `use` declarations | yes | no | no | no | no |
| Circuit declarations | yes | no | no | no | no |
| Top-level pure bindings | yes | no | no | no | no |
| `record`, `material`, `observation`, `evidence`, and `event` | yes | no | no | no | no |
| Biological `part` declarations | syntax pending | no | no | no | no |
| Tagged `outcome` declarations | yes | no | no | no | no |
| Workflow declarations | yes | no | no | no | no |
| Pure workflow bindings | yes | no | no | no | no |
| Durable bindings with `<-` | yes | no | no | no | no |
| `return` | yes | no | no | no | no |
| `match` / `case` | yes | no | no | no | no |
| `if` / `else` and `for` / `in` | yes | no | no | no | no |
| `when every` / `when after` | yes | no | no | no | no |
| Event emission | yes | no | no | no | no |
| Durable workflow runtime | no | no | no | no | no |

The first executable subset deliberately requires one directly specified
plasmid. Other declarations may be parsed alongside it, but compilation reports
an explicit unsupported-feature diagnostic rather than silently ignoring them.
