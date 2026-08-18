# 0037 — Robot learning is a physics projection of reviewed handoffs

## Status

Accepted.

## Context

The workcell planner already states every physical movement as an explicit,
reviewed handoff between named stations. The semantic scene gives those
stations and their labware stable identities, while `lab simulate` interprets
the same run documents to estimate workflow time and operator attention.

An RL environment needs a different model: rigid bodies, collision meshes,
actuators, observations, actions, rewards, termination conditions, randomized
parameters, and thousands of independent episode clocks. Adding those fields
to a workcell plan would make laboratory intent depend on one robot and one
physics engine. Reusing `lab.sim-trace.v0` for policy rollouts would also blur
an operational schedule with high-rate training telemetry.

## Decision

A robot-learning task is a projection of exactly one reviewed handoff. The
versioned `lab.robot-task.v0` document records the source plan node, its
dependencies, the named labware object, source and destination stations,
semantic scene-node identities, operator instructions, and the semantic
completion relation. `lab robot task` emits the projection only after all
three identities resolve uniquely with the right kinds in `lab.scene.v0`.

Everything that depends on the physical embodiment lives in a simulator
binding beside, not inside, that task: robot and controller, collision shape,
mass and friction, calibrated poses, reset variation, measurement tolerances,
physics rate, and episode count. A binding must state its calibration status
and provenance. A hand-authored proxy is useful for software integration but
is not real-to-sim calibration.

Isaac Lab is the first projection. Its prototype adapts the manager-based
Franka relative-IK lift environment to a gripper-compatible plate proxy, a
fixed source and destination, reset-position variation, and a terminal
condition that requires pose tolerance, low linear and angular velocity, and a
released gripper. The proxy is intentionally smaller than an SBS plate because
the stock Franka demo does not establish a qualified full-plate grasp. Isaac
imports stay behind the adapter so contract checks run without a GPU; the
actual PhysX smoke gate runs only in a supported Isaac Lab Linux/CUDA
environment.

Policy trajectories and training metrics will gain their own episode format
when persistence is needed. They are not `lab.sim-trace.v0`. Deployment is
also a separate reviewed step: a learned transport policy may later implement
a workcell transport station, but it does not silently replace the handoff in
an approved plan.

## Consequences

The scientific workflow remains portable while robot embodiments and physics
models can evolve independently. One handoff can be tested with several arms,
controllers, scene captures, and calibrated bindings without recompiling the
experiment. Plan identity survives through training, making later evaluation
and deployment provenance possible.

The first prototype is deliberately not a digital twin. Replacing its proxy
geometry requires measured station frames, plate and gripper assets with
collision meshes, dynamics calibration, and a real-to-sim capture pipeline.
Those improvements refine the binding and facility assets rather than changing
what the workcell plan means.
