# 0007: Tool and compiler CLI boundary

Status: accepted, initial implementation

`labc` is the minimal compiler-development interface. It accepts one source file and exposes compiler representations for inspection. Package management, project scaffolding, laboratory configuration, authentication, and live run operations do not belong in `labc`.

`lab` is the primary interface to Lab. Its initial commands create projects, discover and check package modules, build portable artifacts, and expose package metadata to tools. It is also the future home of dependency management and real workflow operations such as submitting, observing, pausing, and inspecting durable laboratory runs.

`lab` consumes compiler APIs rather than shelling out to `labc`. This keeps diagnostics and typed results structured while allowing `labc` to remain a small, predictable compiler probe.

Editor tooling should integrate through stable machine-facing services owned by the Lab toolchain—initially `lab --json`, and eventually a language server— without parsing human terminal output.
