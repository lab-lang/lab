# 0038 — C3 is the primary compute provider

## Status

Accepted.

## Context

Robot learning needs Linux CUDA machines that can run thousands of parallel
physics environments, retain checkpoints, and support reproducible evaluation.
Those resources are operational infrastructure, not properties of a reviewed
workcell handoff or its `lab.robot-task.v0` projection. The local development
machine also cannot validate the actual Isaac runtime.

C3 provides finite jobs, hardware selection, marketplace routing, locked Python
or Docker environments, content-addressed inputs, machine-readable lifecycle
operations, and collected artifacts. Its job model matches training and
evaluation without requiring Lab to own virtual machines or provider accounts.

Isaac Sim narrows the usable hardware. It requires an RTX-capable GPU, while
A100 and H100 do not have the required RT cores. C3's L40 class is the initial
supported Isaac choice. C3 Docker projects currently accept public Docker Hub
images, while NVIDIA distributes the supported Isaac Lab container from NGC,
so the first runtime uses Isaac Lab's published Python packages and a checked
`uv.lock`.

## Decision

C3 is the first and primary remote compute provider for Lab. A small
`lab-compute` crate owns provider-neutral job states, hardware descriptions,
submission identities, artifact references, and the lifecycle operations Lab
needs: authenticate, list hardware, submit, list jobs, read logs, cancel, and
pull artifacts.

The C3 implementation invokes the installed `c3` CLI and parses only its JSON
automation output. It does not reproduce C3's HTTP API or routing logic.
Credentials remain external to tracked project state. `lab compute doctor`
may read `C3_API_KEY` from an ignored `.env`, verifies authentication and the
current L40 catalog, and never submits a job.

A robot trainer compiles its task, embodiment binding, training configuration,
and runner into a provider-ready project before calling the compute boundary.
The provider does not interpret those files. A returned training manifest
records both the Lab inputs and C3 job, routed provider, and concrete hardware
profile. Checkpoints and metrics are compute artifacts, never
`lab.sim-trace.v0` events.

The first paid gate is a bounded L40 capability probe using stable Isaac Lab
2.3.2, Isaac Sim 5.1, Python 3.11, CUDA PyTorch 2.7, and the repository's
existing PhysX smoke environment. No training command becomes supported until
that gate runs successfully on C3.

## Consequences

The common interface follows a real provider's batch and artifact semantics
instead of speculating about generic clouds. Another provider can be added
later without changing robot tasks or training manifests, but it must satisfy
the same observable lifecycle.

C3 availability, pricing, credentials, and billing stay C3 responsibilities.
Lab preserves the provider's raw status alongside its normalized state and
does not silently resubmit failed work. Resume and evaluation may mount prior
C3 job artifacts server-to-server rather than downloading large checkpoints
through the local machine.

The locked Python runtime avoids the Docker Hub versus NGC registry mismatch,
but it does not prove C3 host compatibility. Driver, GLIBC, system memory,
disk, first-run extension access, and headless PhysX remain explicitly
unverified until the capability artifact exists.
