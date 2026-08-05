# 0001: Minimal language kernel

Status: accepted, partially implemented

Lab uses indentation for behavioral and declaration blocks, `name: value` for declaration properties, braces for constructed data, `=` for pure evaluation, and `<-` for durable effects.

The initial control vocabulary is deliberately small: `if`, `else`, `for`, `in`, `match`, `case`, and `return`. Reactive orchestration adds `when` and `emit`. Concurrency syntax remains an open design problem. Laboratory declaration kinds carry semantic meaning rather than acting as decorative aliases for a generic `type` declaration.

Domain operations such as `synthesize`, `sequence`, `dispose`, `notify`, and `quarantine` are typed library operations, not language keywords. Failure and retry policies are expressed through outcomes, `match`, and bounded iteration rather than a growing policy sublanguage.
