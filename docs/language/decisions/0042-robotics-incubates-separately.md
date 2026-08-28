# 0042: Robotics incubates separately

## Status

Accepted.

## Context

Lab's language, compiler, package model, SBOLInventory facility graph, instrument adapters, reviewed run documents, and execution path are established parts of one stack. General workflow simulation, semantic scenes, photoreal rendering, embodied robot tasks, physics integrations, and remote training compute are much earlier experiments with a different release cadence.

Keeping those experiments in this repository made Lab's stable boundaries appear contingent on an immature robotics architecture and expanded its build, test, documentation, and dependency surface.

## Decision

Simulation, visualization, embodied robotics, and their compute control plane incubate in the separate [`lab-lang/robotics`](https://github.com/lab-lang/robotics) repository.

Lab retains concrete laboratory automation adapters, SBOLInventory facility ingestion, capability allocation, reviewed run-document formats, dry-run review, semantic no-hardware execution, live instrument execution, and human-confirmed facility coordination. Robotics may consume those stable outputs across a repository boundary, but Lab does not host robotics-specific formats, commands, assets, viewers, physics adapters, or compute providers. `lab run --simulate` validates and walks an exact reviewed facility plan without hardware; it is not a physics, scene, or embodied-robotics simulator.

## Consequences

The Lab workspace and CLI stay centered on compiling and operating laboratory workflows. Robotics can evolve or be replaced without changing Lab's package and runtime contracts. Any future integration must begin from an explicit, versioned boundary rather than shared internal modules.
