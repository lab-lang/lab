# Author a pipetting method in Python

Use `from lab.procedures import pipetting as p` to describe liquid operations with typed Python objects. In Lab, a method describes how to carry out a scientific action. This API builds a portable procedure template for a method's liquid-handling task. You choose logical inputs, outputs, volumes, and fluid-path constraints; facility planning selects resources, and an adapter assigns physical wells and emits device instructions.

This API is available in the development checkout. Install that checkout's SDK before running the examples:

```sh
cd crates/lab-python
uv sync --locked --all-groups
```

Commands below run from the Lab repository root using `crates/lab-python/.venv/bin/python`. Consumers of the generated method JSON need a compatible Lab compiler, but do not need to execute the Python authoring script.

## Describe the liquid operations

This example moves one sample into a product vessel and mixes it on the same continuous fluid path:

```python
from lab import methods as m
from lab.procedures import pipetting as p

program = p.Template()
sample_volume = p.volume_parameter("sample_volume")
mix_volume = p.volume_parameter("mix_volume")
mix_cycles = p.integer_parameter("mix_cycles")

sample = program.input(
    "sample", port=0, positions=1, initial_volume=sample_volume
)
prepared = program.product("prepared", output="prepared", positions=1)

with program.path("sample-path", policy=p.ISOLATED_DESTINATIONS) as path:
    path.transfer(
        "move-sample",
        sample.position(0),
        prepared.position(0),
        volume=sample_volume,
    )
    path.mix(
        "mix-sample", prepared.position(0), volume=mix_volume, cycles=mix_cycles
    )

execution = m.TemplateExecution(
    contract=p.CONTRACT,
    body=program.to_template(),
    policy=m.ExecutionPolicy(
        accepted_control_modes=(m.ControlMode.REVIEWED_FILE,)
    ),
)
```

`sample` and `prepared` are logical vessels. Position `0` is the first position in the declared vessel, with no assigned plate, well, deck slot, or instrument. Handles belong to one template, and position lookup checks the declared extent.

`volume_parameter` and `integer_parameter` reference **procedure parameters declared on the enclosing method task**. They do not declare values or read Python variables during compilation. A volume reference must resolve to a positive scalar in canonical QUDT microlitres; a cycle reference must resolve to a unitless integer, and the pipetting contract requires a positive cycle count. For a fixed volume, use `p.microlitres("10.5")`. Decimal strings, integers, and `Decimal` values preserve exact quantities; binary floats are not accepted by this helper.

The `with` block gives its steps one stable fluid-path group and keeps them contiguous. An isolated transfer followed by mixing its destination can stay on that path. A block is committed only if it exits successfully. Nested paths, interleaved template operations, reused group IDs, and use of a path after its block closes are rejected. Physical fluid-path feasibility is checked by Rust and the selected adapter.

## Attach the template to a method

A method connects the scientific action to this procedure. In the [complete runnable author](../../examples/contributing/scientific-package/methods/homogenize.py), the task declares:

| Declaration | Meaning |
| --- | --- |
| `refines="contribution_example.science.homogenize"` | Exact action definition implemented by the method |
| Input to the method: `sample` | Incoming material, with its supplied state |
| Task input `ValueReference.method_input("sample")` | The liquid available through procedure port `0` |
| Task output `prepared` | Result material with the requested state |
| Task parameters `sample_volume`, `mix_volume`, `mix_cycles` | Example values of 30 µL, 10 µL, and 3 |
| `TemplateExecution(contract=p.CONTRACT, ...)` | The canonical `PipettingProgramV1` contract |
| Output from the method: `homogenize/prepared` | The task output returned by the method |

The numbers demonstrate the contribution interface. They are example method data rather than a general preparation recipe. The [Lab action declaration](../../examples/contributing/scientific-package/src/science.lab) separately states material ownership and lineage.

Generate and inspect the package catalog:

```sh
crates/lab-python/.venv/bin/python examples/contributing/scientific-package/methods/homogenize.py
cargo run --locked -p lab-cli -- check examples/contributing/scientific-package
```

The author calls `m.MethodCatalog((homogenize_method(),)).write(...)`, which validates through Rust and writes `methods/homogenize.json`. The package loads this ordinary `lab.method-catalog.v2` document:

```toml
[methods]
documents = ["methods/homogenize.json"]
```

Changing the Python author requires regenerating that JSON. Project loading reads the JSON; it does not discover or run method authoring scripts automatically. Commit the author and generated catalog together. No Rust registration or native SDK rebuild is needed when changing a method using this installed API.

## Check the contribution through refinement

Catalog validation checks the method graph and template references. Refinement supplies the checked action and parameter values, renders the template, validates the resulting procedure, and derives its capability requirements. Run the package's Python consumer to exercise that whole portable boundary:

```sh
cargo run --locked -p lab-cli -- bindings python examples/contributing/scientific-package
PYTHONPATH=examples/contributing/scientific-package/bindings/python \
  crates/lab-python/.venv/bin/python examples/contributing/scientific-package/protocol.py
```

The consumer reports that the package action compiled and refined successfully. The package has no inventory or adapter binding, so the example ends at method refinement. To obtain device files, a consuming project must supply compatible inventory, offerings, profiles, and adapter bindings for facility planning.

The [authoring tests](../../crates/lab-python/tests/test_pipetting_authoring.py) check generated-catalog equivalence, identical planning-problem projection, strict Python typing, and Rust rejection of missing parameters, wrong units, invalid cycle counts, and insufficient liquid. Existing Rust contribution tests compare the declarative template with a registered Rust builder producing the same program.

## API reference

| API | Purpose |
| --- | --- |
| `p.Template()` | Begin an ordered, device-neutral template |
| `p.volume_parameter(name)` | Reference a task's positive microlitre parameter |
| `p.integer_parameter(name)` | Reference a task's unitless integer parameter |
| `p.microlitres(value)` | Create an exact, positive literal volume |
| `program.input(name, port=..., initial_volume=..., positions=1)` | Declare an incoming task value; volume is per position |
| `program.product(name, output=..., positions=1)` | Declare an initially empty vessel for a named task output |
| `program.source(name, material=..., positions=1, initial_volume=None)` | Declare a named task material source; its exact lot or upstream value is selected later |
| `program.vessel(name, role=..., ...)` | Declare any canonical vessel role and per-position constraints |
| `vessel.position(index)` / `vessel.positions()` | Obtain one checked logical position or all positions |
| `program.path(name, policy=...)` | Start a continuous fluid-path block |
| `path.transfer(name, source, destination, volume=..., technique=None)` | Transfer between two logical positions |
| `path.mix(name, target, volume=..., cycles=..., technique=None)` | Mix one logical position |
| `program.distribute(name, source, destinations, volume_each=..., policy=..., technique=None)` | Distribute liquid with explicit reuse constraints |
| `program.barrier(name, reason=...)` | State a semantic boundary between operations |
| `program.to_template()` | Return a detached JSON-compatible snapshot for `TemplateExecution.body` |

`ISOLATED_DESTINATIONS` requires destination isolation. `SHARED_SOURCE_NO_REENTRY` permits destinations to share a source-loaded path but forbids source re-entry after destination contact. These policies constrain device realization; they are not vendor tip commands.

For several destinations, use distribution or author one isolated path per destination. Python loops can generate a fixed template at authoring time. Parameter references cannot control Python loops or arithmetic; an algorithm whose shape depends on the checked action at refinement time still uses a [registered Rust builder](extensions.md#contribute-a-pipetting-algorithm).

## Vessel constraints and techniques

The existing typed classes in `lab.procedures` also describe roles, temperature ranges, and portable liquid-access techniques. `program.vessel` supports `ProcedureInputVesselRole`, `MaterialSourceVesselRole`, `ProductVesselRole`, `InputOutputVesselRole`, `MaterialProductVesselRole`, and `IntermediateVesselRole`. Material and output roles generate the appropriate checked task-reference slots automatically.

Use `initial_volume`, `working_capacity`, and `dead_volume` for per-position quantities. Omit an unknown quantity; a stated volume must be positive. `temperature` takes a `TemperatureRange`. The compiler enforces the method's stated constraints, and adapters additionally enforce the selected physical labware's constraints.

```python
from decimal import Decimal

from lab import procedures as views
from lab.procedures import pipetting as p

program = p.Template()
sample = program.vessel(
    "sample",
    role=views.InputOutputVesselRole(input=0, output="prepared"),
    initial_volume=p.microlitres(30),
    working_capacity=p.microlitres(50),
    dead_volume=p.microlitres(5),
    temperature=views.TemperatureRange(
        views.Temperature(Decimal(4)),
        views.Temperature(Decimal(8)),
    ),
)
with program.path("mix-path", policy=p.ISOLATED_DESTINATIONS) as path:
    path.mix(
        "mix",
        sample.position(0),
        volume=p.microlitres(10),
        cycles=3,
        technique=views.MixTechnique(
            aspiration=views.TrackedLiquidSurfaceAspiration()
        ),
    )
```

Transfers and distributions accept `TransferTechnique`; mixing accepts `MixTechnique`. These preserve aspiration and dispense strategies, blow-out, and touch-tip constraints; transfer techniques also support a literal air gap. Omit `technique` to use the canonical default. A valid template may still require features that a particular adapter cannot implement.

## Ownership of the implementation

Python owns typed construction, local handle checks, and serialization. Rust owns task binding, unit validation, liquid accounting, fluid-path semantics, capability derivation, method refinement, and allocation. Adapters own physical feasibility and device emission. Runtime executes reviewed documents.

`to_template()` does not claim that a program is valid or executable. `MethodCatalog.write()` does not resolve every parameter or qualify a physical protocol. This API adds no new wire format, Python execution callback, planner, or device driver. Existing imports such as `from lab.procedures import PipettingProgramV1, parse_program` continue to work for inspecting compiler output.
