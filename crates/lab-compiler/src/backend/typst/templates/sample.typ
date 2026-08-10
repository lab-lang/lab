// A representative document for iterating on lab-style.typ typography
// without running the Lab toolchain: `typst compile sample.typ` (any typst
// CLI ≥ 0.13). It exercises every construct the renderer emits: the heading
// ladder with labels, notices, tables with numeric columns, lists, code
// spans, and the µ / ° / → glyphs protocols rely on.

#import "lab-style.typ": hl, lab-table, notice, protocol-doc

#show: protocol-doc.with(
  title: "Automated plasmid build",
  subtitle: "Operator manual for one robot session",
  target: "hamilton-star",
  instrument: "Hamilton STAR",
  version: "0.0.0-sample",
)

#notice[Generated concept protocol. Review and qualify every run for the actual laboratory before execution.]

= Build summary

Workflow: Golden Gate assembly, heat-shock transformation, then serial
dilution and selective plating.

- Plasmids assembled: 2
- Strains built: 1

= #hl("Stage 1")Golden Gate assembly

Keep DNA and enzymes cold. For every reaction, add reagents in the order
shown. Incubate at 4 °C, heat shock at 45 °C.

== #raw("p_gfp")

#lab-table(
  align: (left, right),
  flex: 0,
  header: ([Reagent], [Volume per reaction]),
  [Nuclease-free water], [16 µL],
  [T4 DNA ligase buffer], [3 µL],
  [#raw("BsaI")], [1 µL],
  [*Total*], [*30 µL*],
)

=== Reaction wells

Wells A1, A2 on the reaction plate; see #raw("assembly_run.star.json").

==== Then, by hand (seal the plate)

+ Seal the reaction plate.
+ Carry it to the thermocycler; close the door.
