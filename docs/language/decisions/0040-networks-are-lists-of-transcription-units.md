# 0040 — A genetic network is a list of transcription units

## Status

Accepted.

## Context

`Circuit<Trigger, Product>` describes one promoter driving one coding sequence:
a single transcription unit, with a `layout:` of the parts it is built from.
Most circuits worth building are not one transcription unit. A cascade is two,
a repressilator is three in a ring, a logic gate integrates several inputs, and
a panel expresses several reporters at once.

The obvious reading of that gap is that `Circuit` is too small, and that a
network needs a bigger type: more parameters, a variadic form, or a graph
structure in the body. Each of those makes the type describe the wiring, which
means the wiring stops being checked and starts being data.

It is also the shape the field already uses. A LOICA network is a set of
operators; its own SBOL export emits one transcription-unit `Component` per
operator, wired by shared gene products, with regulators typed `SBO:0000252`
and roled `SO:0003700`. Nothing in that model is a bigger circuit.

The piece Lab was missing is smaller than a new type. A regulator is a protein
that another promoter answers to, so it plays `Protein` and `Signal` at once,
and a record has always been able to play several roles. Once it does, the
cascade is checked by the ordinary rules: the unit expressing `TetR` produces
`Circuit<_, TetR>`, the unit answering to it consumes `Promoter<TetR>`, and
wiring them to the wrong regulator is a type error.

## Decision

A network is a list of circuits, one per transcription unit, and the wiring
between them is carried by the types of the gene products they share.

```lab
record TetR is Protein, Signal

stage_one = unit(p_lac, tetR_cds)
stage_two = unit(p_tet, gfp_cds)

ring: List<Circuit<any Signal, any Protein>> = [stage_one, stage_two]
```

Four consequences follow, and each is a small addition rather than a new
concept.

**A plasmid's cargo is a list.** `cargo?: List<Circuit<any Signal, any
Protein>>`. A plasmid carrying a network carries several units, and the
triggers are forgotten because units with different triggers have no trigger in
common, which is what decision 0017 already says about a panel.

**A promoter states which way it answers.** `regulation?: Regulation`, whose
values are `induced` and `repressed`. The signal a promoter responds to and the
direction it responds in are different facts, and the second is the difference
between a buffer and an inverter. LOICA leaves it implicit in the Hill
parameters, where a basal rate above the regulated rate means repression; Lab
states it, because a reader should not have to compare two numbers to learn
whether a circuit inverts.

**Several signals are one signal.** `record Both<First: Signal, Second: Signal>
is Signal` gives a promoter that integrates two inputs something to respond to,
and nesting states more than two. A condition made of several signals is itself
a condition, so it plays the same role its parts do.

**Several products are one product.** `record Operon<First: Protein, Second:
Protein> is Protein`, for a transcription unit carrying more than one coding
sequence. Everything downstream of the promoter is expressed together, which is
what an operon is.

`Both` and `Operon` required a compiler fix rather than a language change: a
parameterized type could never play a role, because both role checks demanded
the type carry no arguments. That also meant `Reading<Fluorescence>` was not
`Evidential` despite `Reading` being declared so, which was a defect on its own
terms. A declaration states the roles it plays, and its arguments do not bear on
the question.

## Consequences

A network is checked the way a single circuit always was, and the check is
worth more: the compiler rejects a cascade wired to the wrong regulator, and a
ring that does not close.

The units of a network are not ordered by the list. A `layout:` orders the
parts within one transcription unit, and the list says which units there are,
not where they sit on a plasmid. Physical order across units remains the
plasmid's `components`.

`Circuit`'s two parameters stay unbounded, so `Circuit<Integer, String>` is
still accepted by the frontend. Bounding them belongs with the wider question
of bounds on standard nominals.

What a network does not yet carry is the numbers. Hill parameters arrive on a
characterized LOICA operator and have nowhere to live in a catalogued
promoter's schema, so they stay outside the Lab module. A characterization
schema is the natural next step, and `regulation` is the first field of it.
