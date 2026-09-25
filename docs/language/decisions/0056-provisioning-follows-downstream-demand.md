# 0056: Provisioning follows downstream material demand

## Status

Accepted. Implements the provisioning part of [0054](0054-every-material-carries-a-quantity.md) without changing surface syntax.

## Decision

`cells <- provision DH5alpha` reserves enough stock for the canonical liquid procedure that consumes `cells`. The planner follows the material value edge to its provisioning call and uses the liquid ledger's required starting volume and withdrawals. Recipes state per-reaction quantities and replicate counts; total demand is derived.

Physical stock packaging belongs to the inventory lot. Lab defines three optional extension predicates in `https://www.lab-compiler.org/ns/inventory#`: `aliquotVolumeUl` is the exact positive fill volume of each aliquot, `aliquotCount` is the number of available aliquots, and optional `deadVolumeUl` is the nonnegative inaccessible volume of each aliquot. Volumes are exact decimal microlitres. Declaring any of these requires both fill volume and count. A zero count means unavailable stock.

Each provisioning call reserves separate zero-based aliquot positions within a selected lot. The planner never combines independent calls' demands into one shared aliquot. It may bind one call to several aliquots, splitting distributions at stock boundaries while keeping each destination's dose unchanged. It does not pool aliquots or split a single transfer between them. Every source mix must still be valid after binding; incompatible mixing or instrument constraints are planning errors.

The facility solution records the inferred demand, actual withdrawal, selected lot, aliquot fill and dead volume, and reserved positions. Allocation freezes the same reservations into LAIR and binds the corresponding canonical source vessels and steps. Adapter feasibility is checked again against the bound sources. Reviewed plans and manual run sheets retain the staging instructions. CLI planning/build output and Python's `FacilityPlan.material_requirements` expose this same record.

These reservations are within one immutable plan. Planning neither mutates inventory nor prevents another independent run from selecting the same stock; live reservation coordination remains execution/inventory-service work.

## Scope

Inference currently follows direct Method input edges from an explicit `provision` output into canonical pipetting source vessels. General quantity-carrying materials, forwarding through arbitrary procedures, and `draw` remain separate work under 0054. Manual procedures without canonical liquid semantics do not acquire invented volume demands. Existing Methods with explicit source fills may retain those loads when their lots have no stock annotations; open provisioned sources require stock evidence.

Stock binding uses deterministic first-fit allocation after Method and Asset selection. One call draws from one selected lot, possibly using several of that lot's uniform aliquots. It does not globally search alternative lot assignments, Method choices, or instrument layouts to resolve a shortage; such coupled search remains separate work under 0055.
