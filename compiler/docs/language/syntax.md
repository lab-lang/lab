# Surface syntax

## Layout

Lab uses newlines and indentation for statement blocks. A colon introduces an
indented block. Tabs are not accepted for indentation.

Braces construct or destructure data; they never delimit control-flow blocks.
Parentheses call pure functions and group expressions. Brackets construct
collections. Angle brackets delimit type arguments.

Line comments begin with `#`.

## Current vocabulary

The kernel keeps orchestration mechanics distinct from domain operations:

| Role | Words or forms |
| --- | --- |
| Modules | `use` |
| Biological declarations | `part`, `circuit`, `plasmid` |
| Laboratory data declarations | `record`, `material`, `observation`, `evidence`, `event`, `outcome` |
| Orchestration declaration | `workflow` |
| Signatures and contracts | `input`, `output`, `require`, `accept` |
| Control | `if`, `else`, `for`, `in`, `match`, `case`, `return` |
| Reactive control | `when`, `every`, `after`, `emit` |
| Boolean operators | `and`, `or`, `not` |

Most of these are contextual: for example, `input` has declaration meaning in
a circuit or workflow body. Laboratory verbs such as `synthesize`, `assemble`,
`sequence`, `store`, and `dispose` are library operations, not keywords.

The core punctuation has one job each:

| Form | Meaning |
| --- | --- |
| `:` plus indentation | declaration or control block |
| `{ name: value }` | construct or destructure typed data |
| `(...)` | call or group |
| `[...]` | collection literal |
| `<...>` | type arguments |
| `=` | deterministic evaluation or state transition |
| `<-` | durable physical or external effect |
| `==`, `!=`, `<`, `<=`, `>`, `>=` | comparison |
| `+`, `-`, `*`, `/`, `..` | arithmetic, composition, or range operations selected by types |

## Bindings

```lab
expected = design.sequence
observed <- sequence aliquot
<- dispose aliquot
```

`=` performs deterministic language evaluation. `<-` performs a durable effect
and binds its recorded result. A bare `<-` performs an effect whose result is
not retained.

## Laboratory declarations

```text
part         reusable biological part
circuit      reusable biological organization
plasmid      physical artifact design
record       ordinary immutable structured data
observation  recorded measurement or inspection
evidence     information evaluated against a claim
event        durable orchestration occurrence
outcome      tagged workflow result
workflow     durable orchestration definition
```

The declaration words are meaningful only if the compiler assigns different
laws to their values.

## Plasmid requirements and acceptance

```lab
plasmid p_sensor:
  sequence = dna("ATGCGTACGTTAGCTA")

  require topology == circular

  accept sequence == design.sequence
  accept concentration >= 100 ng/uL
  accept volume >= 20 uL
```

`require` is checked before physical construction. `accept` describes a runtime
claim that must be supported by evidence.

## Data construction

```lab
return Rejected{
  material: retained,
  reason: sequence_mismatch,
  evidence: evidence,
}
```

Record fields use `name: value`; multiline constructors conventionally retain a
trailing comma.

## Reactive clauses

```lab
when every 30 min:
  image <- capture image of plate

when after 18 h:
  return TimedOut{
    plate: plate,
    observations: observations,
  }
```

`when` introduces an event pattern. `every` is a periodic timer pattern and
`after` is a one-shot timer pattern.
