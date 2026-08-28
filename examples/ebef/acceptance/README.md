# EBEF-derived facility acceptance

This fixture extends the public-data EBEF catalog with two explicitly synthetic, no-hardware Assets. The Assets are digital twins shaped by the cataloged Microlab Prep and Epoch 2, but they do not change the physical Assets' `Described` qualification or `UnspecifiedControl` mode.

The acceptance test materializes the public catalog and this RDF extension as one Profile 0.2 document, binds liquid handling, incubation, and absorbance requirements to exact `Simulatable` offerings, moves one exact MaterialLot between the bound Assets, and executes reviewed `lab.simulation-run.v1` documents through the `lab.simulator` adapter.

The test proves eager preflight, deterministic multi-Asset execution, exact resume behavior, source-inventory immutability, mode-bound ledgers, and conformant simulation provenance. It performs no hardware I/O and does not assert that Lab can control the installed EBEF instruments.
