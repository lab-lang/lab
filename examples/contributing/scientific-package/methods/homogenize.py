"""Author the portable homogenization Method and regenerate its checked-in catalog.

Run this file with the development Lab SDK installed. The emitted JSON is sufficient for
consumers of this package; they do not run this authoring script during compilation.
"""

from pathlib import Path

from lab import methods as m
from lab.procedures import MICROLITRE
from lab.procedures import pipetting as p


def pipetting_template() -> p.Template:
    program = p.Template()
    sample_volume = p.volume_parameter("sample_volume")
    mix_volume = p.volume_parameter("mix_volume")
    mix_cycles = p.integer_parameter("mix_cycles")

    sample = program.input("sample", port=0, positions=1, initial_volume=sample_volume)
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
    return program


def parameter(
    name: str, value: m.Scalar, unit: str | None = None
) -> m.ProcedureParameter:
    return m.ProcedureParameter(
        name,
        f"https://example.org/parameter#{name}",
        m.ProcedureValueExpression.constant(
            m.ProcedureValue.scalar(m.PropertyValue(value, unit))
        ),
    )


def homogenize_method() -> m.Method:
    return m.Method(
        id="https://example.org/method#homogenize",
        refines="contribution_example.science.homogenize",
        inputs=(m.MethodInput("sample", m.Port.material_as_supplied()),),
        tasks=(
            m.Task(
                id="homogenize",
                operation="https://example.org/procedure#Homogenize",
                inputs=(m.ValueReference.method_input("sample"),),
                outputs=(m.TaskOutput("prepared", m.Port.material_as_requested()),),
                parameters=(
                    parameter("sample_volume", m.Scalar.real("30"), MICROLITRE),
                    parameter("mix_volume", m.Scalar.real("10"), MICROLITRE),
                    parameter("mix_cycles", m.Scalar.integer(3)),
                ),
                execution=m.TemplateExecution(
                    contract=p.CONTRACT,
                    body=pipetting_template().to_template(),
                    policy=m.ExecutionPolicy(
                        accepted_control_modes=(m.ControlMode.REVIEWED_FILE,),
                    ),
                ),
            ),
        ),
        outputs=(
            m.MethodOutput(
                "prepared", m.ValueReference.task_output("homogenize", "prepared")
            ),
        ),
    )


if __name__ == "__main__":
    output = m.MethodCatalog((homogenize_method(),)).write(
        Path(__file__).with_suffix(".json")
    )
    print(f"Validated Method catalog: {output}")
