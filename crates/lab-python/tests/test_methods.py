"""Portable Method authoring uses the same Rust contract and refinement as Lab source."""

import json
import tempfile
import unittest
from pathlib import Path

import lab
from lab import methods as m

PLASMID_PRODUCT = "https://www.lab-compiler.org/ns/material-state#PlasmidProduct"
SEQUENCE_SYNTHESIS = "https://example.org/capability#SequenceSynthesis"
SYNTHESIZE = "https://example.org/procedure#SynthesizePlasmid"
ARTIFACT = "https://example.org/procedure#Artifact"
DEPENDENCIES = "https://example.org/procedure#Dependencies"

SOURCE = """\
use std.bio.build
use std.bio.designs

plasmid reporter:
  sequence = dna("ACGT")

workflow main() -> Material<Plasmid>:
  product <- realize reporter
  return product
"""


def sequence_synthesis() -> m.Method:
    """A small third-party realization Method with no facility facts."""

    return m.Method(
        id="https://example.org/method#sequence-synthesis",
        refines="std.bio.build.realize",
        inputs=(m.MethodInput("design", m.Port.design()),),
        parameters=(
            m.MethodParameter.scalar("artifact", m.ScalarType.TEXT),
            m.MethodParameter.list("dependencies", m.ScalarType.TEXT),
        ),
        tasks=(
            m.Task(
                id="synthesize",
                operation=SYNTHESIZE,
                inputs=(m.ValueReference.method_input("design"),),
                outputs=(m.TaskOutput("product", m.Port.material(PLASMID_PRODUCT)),),
                parameters=(
                    m.ProcedureParameter(
                        "artifact",
                        ARTIFACT,
                        m.ProcedureValueExpression.intent_parameter("artifact"),
                    ),
                    m.ProcedureParameter(
                        "dependencies",
                        DEPENDENCIES,
                        m.ProcedureValueExpression.intent_parameter("dependencies"),
                    ),
                ),
                materials=(m.MaterialInput("template", m.MaterialSource.constant("template_dna")),),
                requirements=(
                    m.Requirement(
                        id="synthesis",
                        capability_kind=SEQUENCE_SYNTHESIS,
                        accepted_control_modes=(m.ControlMode.REVIEWED_FILE,),
                    ),
                ),
            ),
        ),
        outputs=(m.MethodOutput("product", m.ValueReference.task_output("synthesize", "product")),),
    )


class MethodTests(unittest.TestCase):
    def test_custom_method_refines_through_the_shared_compiler(self) -> None:
        program = lab.check_sources({"example.main": SOURCE})

        refined = m.refine(
            program,
            methods=(sequence_synthesis(),),
            include_standard=False,
        )

        self.assertIn("https://example.org/method#sequence-synthesis", refined.lair)
        self.assertIn(SEQUENCE_SYNTHESIS, refined.lair)
        self.assertEqual(refined.planning_problem["schema_version"], "lab.planning-problem.v5")
        choices = refined.planning_problem["choices"]
        self.assertEqual(len(choices), 1)
        self.assertEqual(
            choices[0]["candidates"][0]["method"],
            "https://example.org/method#sequence-synthesis",
        )
        parameters = choices[0]["candidates"][0]["tasks"][0]["parameters"]
        self.assertEqual(parameters[0]["value"]["value"]["value"]["value"], "reporter")
        self.assertEqual(
            parameters[1]["value"],
            {"kind": "list", "element_type": "text", "values": []},
        )
        self.assertEqual(
            choices[0]["candidates"][0]["tasks"][0]["materials"],
            [
                {
                    "id": "std-bio-build-realize-0::https://example.org/method#sequence-synthesis::synthesize::material::template",
                    "symbol": "template_dna",
                    "source": {"kind": "inventory"},
                }
            ],
        )

    def test_catalog_validation_is_authoritative_in_rust(self) -> None:
        method = sequence_synthesis()
        invalid = m.Method(
            id=method.id,
            refines=method.refines,
            inputs=method.inputs,
            tasks=(
                m.Task(
                    id="synthesize",
                    operation=SYNTHESIZE,
                    requirements=(),
                ),
            ),
        )

        with self.assertRaisesRegex(ValueError, "has no Capability requirements"):
            m.MethodCatalog((invalid,), include_standard=False).validate()

    def test_catalog_writes_the_versioned_package_document(self) -> None:
        catalog = m.MethodCatalog((sequence_synthesis(),), include_standard=False)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "methods.json"

            written = catalog.write(path)
            document = json.loads(path.read_text(encoding="utf-8"))

        self.assertEqual(written, path)
        self.assertEqual(document["schema_version"], m.METHOD_CATALOG_SCHEMA_VERSION)
        self.assertEqual(document["methods"][0]["id"], sequence_synthesis().id)
        self.assertNotIn("include_standard", document)

    def test_exact_scalars_and_units_match_the_shared_json_contract(self) -> None:
        value = m.PropertyValue(
            m.Scalar.real("0.0100"),
            unit="http://qudt.org/vocab/unit/L",
        )

        self.assertEqual(
            value.to_dict(),
            {
                "value": {"type": "real", "value": "0.0100"},
                "unit": "http://qudt.org/vocab/unit/L",
            },
        )
        with self.assertRaisesRegex(ValueError, "only numeric"):
            m.PropertyValue(m.Scalar.text("ten"), unit="http://qudt.org/vocab/unit/L")


if __name__ == "__main__":
    unittest.main()
