# 0051: Interchangeable physical resources resolve without a pin

## Status

Accepted. Amends [0045: LAIR represents method alternatives before facility allocation](0045-lair-method-refinement-and-facility-allocation.md).

## Context

0045 states that the solver selects only when the facility and stated policy make one solution
unique, and that equally valid solutions remain an explained ambiguity. That rule is right for
Methods, where choosing between two scientifically valid refinements is a decision a person should
make and a reviewer should see.

The implementation generalized the rule to every axis the solver enumerates, including MaterialLots
and Assets. A laboratory holding two active tubes of the same part, which is the ordinary state of a
freezer, could not be planned at all. The failure named no resource, and the only stated remedy,
allocation policy, covered Methods alone. Deactivating a real tube in the inventory to make a
project build is a worse outcome than either choosing or refusing.

## Decision

Ambiguity is refused only where the alternatives differ in a way a reviewer would want to decide.

Two MaterialLots are interchangeable when they are built from the same Component and are both
active in the selected facility. Interchangeable lots do not make a plan ambiguous. The solver binds
one by lot identity, so the choice is stable across runs, and records every lot it did not take on
the same binding. The decision is frozen in the reviewed plan and visible next to the one that was
made, which is what review requires; it is not an arbitrary pick made silently at run time.

Two Assets are not interchangeable. Each carries its own calibration and its own place on a bench,
and a plan that silently moved work between them would not be reproducible. When more than one Asset
can satisfy a requirement, planning refuses and the package names the one to use.

`[[planning.assets]]` states that choice. With only an `asset`, it binds every requirement that
Asset can serve; `capability-kind` narrows that preference. Those preferences restrict the field
only where the named Asset is genuinely eligible, so one stated for the instruments does not make
manual work at a workstation infeasible. A `requirement` pin is an exact reviewed binding and makes
its Method infeasible when the named Asset cannot serve it; it never silently selects another Asset.

An ambiguity that is refused explains itself: it names the choices, materials, or requirements on
which two complete plans disagree, and prints the policy entry that resolves it.

## Consequences

- An ordinary shelf, with more than one tube of a reagent, plans without configuration.
- The lots that were not used stay on the reviewed plan, so a reviewer sees the alternatives rather
  than having to reconstruct them from the inventory.
- Choosing between physical instruments stays a laboratory decision, and stays explicit.
- The uniqueness rule in 0045 continues to govern Method selection unchanged.
- Interchangeability is a property of the resources, not a policy setting, so a facility cannot
  configure away the distinction between a fungible reagent and a particular instrument.
