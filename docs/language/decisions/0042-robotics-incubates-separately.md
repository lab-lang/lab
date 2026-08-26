# 0042: Robotics incubates separately

## Status

Accepted.

## Context

Lab's language, compiler, package model, instrument backends, reviewed run documents, and live execution path are established parts of one stack. General workflow simulation, facility models, semantic scenes, photoreal rendering, embodied robot tasks, physics integrations, and remote training compute are much earlier experiments with a different release cadence.

Keeping those experiments in this repository made Lab's stable boundaries appear contingent on an immature robotics architecture and expanded its build, test, documentation, and dependency surface.

## Decision

Simulation, visualization, embodied robotics, and their compute control plane incubate in the separate [`lab-lang/robotics`](https://github.com/lab-lang/robotics) repository.

Lab retains concrete laboratory automation backends, compiler-owned target profiles, reviewed run-document formats, dry-run review, live instrument execution, and human-confirmed workcell coordination. Robotics may consume those stable outputs across a repository boundary, but Lab does not host robotics-specific formats, commands, assets, viewers, physics adapters, or compute providers.

## Consequences

The Lab workspace and CLI stay centered on compiling and operating laboratory workflows. Robotics can evolve or be replaced without changing Lab's package and runtime contracts. Any future integration must begin from an explicit, versioned boundary rather than shared internal modules.
