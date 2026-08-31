# lab-capability

`lab-capability` owns the RDF-free semantic types shared by Lab frontends, LAIR, method definitions, constraint solving, adapter invocations, reviewed plans, and Python bindings.

It deliberately does not depend on `sbol-inventory`, `sbol3`, Pliron, the Lab language frontend, or an instrument crate. Those systems meet at explicit conversion boundaries:

- SBOLInventory readers convert profile qualification, control modes, property values, and IRIs into these semantic types.
- LAIR operations carry these types without importing RDF graphs.
- planners compare typed requirements and observed offering facts exactly, then retain selected SBOLInventory identities as IRIs.
- adapter and Python APIs serialize these types as versioned ordinary data without exposing compiler pointers or RDF implementation details.

Semantic identities use validated absolute IRIs. Numeric capability values use canonical arbitrary-precision decimal strings, so allocation does not turn RDF decimals into binary floating-point values.
