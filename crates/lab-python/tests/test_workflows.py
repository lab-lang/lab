"""Workflows written as Python functions.

The reporter program under `programs/reporter/` states a build, a reactive
observation, and an entry point that ties them together. The hand-written Lab
below is the same program in Lab's own syntax, and the two must compile to the
same checked module: a workflow read from Python syntax is a way of writing
Lab, not a second dialect of it.
"""

import re
import unittest
from typing import Any

import lab
from programs.reporter import main as reporter_main
from programs.reporter import observe as reporter_observe
from programs.reporter import plasmid as reporter_plasmid
from programs.reporter import workflow as reporter_workflow

OBSERVE = """\
/*!
 * Watch a plate until it has something worth picking from.
 */

use std.bio.designs
use std.lab.plasmid

/** One image, what was counted in it, and how long the plate had grown. */
record PlateObservation is Evidential:
  image: Image
  colonies: ColonyMap
  elapsed: Duration

/** What watching a plate produced. */
record ColonyGrowth:
  plate: Material<Medium is inoculated>
  observations: List<PlateObservation>

  case Ready:
    colonies: ColonyMap

  case TimedOut

/** Image every half hour, and stop at the first plate worth picking from. */
workflow grow_colonies(
  plate: Material<Medium is inoculated>,
) -> ColonyGrowth:
  state observations: List<PlateObservation> = []

  when every 30 min:
    image <- capture image of plate
    colonies = detect_colonies(image)
    observations = observations + [PlateObservation{
      image: image,
      colonies: colonies,
      elapsed: workflow.elapsed,
    }]

    if colonies.isolated.count >= 8:
      return Ready{
        plate: plate,
        colonies: colonies,
        observations: observations,
      }

  when after 18 h:
    return TimedOut{plate: plate, observations: observations}
"""

_OFFSET = re.compile(r"^(?P<name>.+)@\d+$")


def normalize(value: Any) -> Any:
    """Checked IR with source positions removed."""

    if isinstance(value, dict):
        if set(value) == {"module", "local"} and isinstance(value["local"], str):
            matched = _OFFSET.match(value["local"])
            if matched:
                return {**value, "local": matched.group("name")}
        return {key: normalize(item) for key, item in value.items()}
    if isinstance(value, list):
        return [normalize(item) for item in value]
    return value


#: The program, in the order its modules depend on each other.
MODULES = (
    reporter_plasmid.module,
    reporter_workflow.module,
    reporter_observe.module,
    reporter_main.module,
)


def declarations(checked: dict[str, Any], kind: str) -> dict[str, Any]:
    return {
        declaration["name"]: declaration
        for declaration in checked["declarations"]
        if declaration["kind"] == kind
    }


class ProgramTests(unittest.TestCase):
    def test_the_whole_program_checks(self) -> None:
        program = lab.check(*MODULES)

        self.assertEqual(len(program.checked), 4)

    def test_the_reactive_workflow_matches_the_hand_written_lab(self) -> None:
        written = lab.check_sources({"reporter.observe": OBSERVE})
        emitted = lab.check(*MODULES)

        self.assertEqual(
            normalize(emitted.checked["reporter.observe"]),
            normalize(written.checked["reporter.observe"]),
        )


class TranslationTests(unittest.TestCase):
    """What each Python form becomes, read off the emitted Lab."""

    def setUp(self) -> None:
        self.build = reporter_workflow.module.source()
        self.observe = reporter_observe.module.source()
        self.main = reporter_main.module.source()

    def test_perform_is_the_durable_arrow(self) -> None:
        self.assertIn("product <- realize reporter", self.build)
        self.assertIn("culture <- dilute culture", self.build)

    def test_assignment_is_a_pure_binding(self) -> None:
        self.assertIn("colonies = detect_colonies(image)", self.observe)

    def test_a_step_binding_several_results_binds_them_all(self) -> None:
        self.assertIn(
            "strain, culture <- transform reporter_host from plasmids into cells",
            self.build,
        )

    def test_an_operand_built_in_place_is_bound_above_the_step(self) -> None:
        # An action operand is one word in Lab, so the list of materials is
        # named before the step that takes it.
        self.assertIn("plasmids = [product]", self.build)

    def test_an_optional_clause_is_left_out_when_its_operand_is(self) -> None:
        self.assertIn("realize reporter\n", self.build)
        self.assertNotIn("realize reporter from", self.build)

    def test_a_quantity_operand_keeps_its_unit(self) -> None:
        self.assertIn("culture <- recover culture for 1 h", self.build)

    def test_state_is_declared_with_its_type(self) -> None:
        self.assertIn("state observations: List<PlateObservation> = []", self.observe)

    def test_timers_become_when_blocks(self) -> None:
        self.assertIn("when every 30 min:", self.observe)
        self.assertIn("when after 18 h:", self.observe)

    def test_appending_to_state_rebinds_it(self) -> None:
        self.assertIn("observations = observations + [", self.observe)

    def test_the_context_is_read_as_the_workflow(self) -> None:
        self.assertIn("elapsed: workflow.elapsed", self.observe)

    def test_a_case_is_returned_by_its_bare_name(self) -> None:
        self.assertIn("return Ready{", self.observe)
        self.assertIn("return TimedOut{", self.observe)

    def test_a_match_arm_names_the_case_without_its_record(self) -> None:
        self.assertIn("case Ready:", self.main)
        self.assertIn("case TimedOut:", self.main)

    def test_emit_states_the_event(self) -> None:
        self.assertIn("emit ColoniesReady{colonies: growth.colonies}", self.main)

    def test_a_step_with_no_result_is_written_bare(self) -> None:
        self.assertIn("<- dispose growth.plate", self.main)

    def test_one_workflow_performs_another(self) -> None:
        self.assertIn("strain, plate <- build_reporter", self.main)
        self.assertIn("growth <- grow_colonies plate", self.main)

    def test_several_results_are_named_after_what_the_body_returns(self) -> None:
        self.assertIn("strain: Material<Strain>,", self.build)
        self.assertIn("plate: Material<Medium is inoculated>,", self.build)

    def test_one_result_needs_no_name(self) -> None:
        self.assertIn("-> ColonyGrowth:", self.observe)

    def test_a_record_states_its_roles_and_fields(self) -> None:
        self.assertIn("record PlateObservation is Evidential:", self.observe)
        self.assertIn("record ColoniesReady is Event:", self.main)

    def test_a_case_with_no_fields_has_no_block(self) -> None:
        self.assertIn("case TimedOut\n", self.observe)

    def test_imports_are_inferred_from_what_the_body_mentions(self) -> None:
        self.assertIn("use std.lab.plasmid", self.observe)
        self.assertIn("use reporter.workflow", self.main)
        self.assertIn("use reporter.observe", self.main)


class RefusalTests(unittest.TestCase):
    """A form with no Lab meaning is refused where it is written."""

    def _translate(self, body: str, **rest: str) -> None:
        namespace: dict[str, Any] = {}
        source = (
            "import lab\n"
            "from lab import Material, Strain\n"
            "from lab.bio.designs import Medium, inoculated\n"
            'module = lab.Module("refusal.demo")\n'
            "@lab.workflow\n" + body
        )
        path = f"/tmp/lab_refusal_{abs(hash(body))}.py"
        with open(path, "w") as handle:
            handle.write(source)
        namespace["__name__"] = "refusal_demo"
        namespace["__file__"] = path
        import runpy

        runpy.run_path(path, run_name="refusal_demo")

    def test_a_statement_with_no_lab_form_is_refused(self) -> None:
        with self.assertRaisesRegex(lab.WorkflowError, "while"):
            self._translate("def w(wf) -> Material[Strain]:\n    while True:\n        pass\n")

    def test_an_untyped_parameter_is_refused(self) -> None:
        with self.assertRaisesRegex(lab.WorkflowError, "has no type"):
            self._translate("def w(wf, plate) -> Material[Strain]:\n    return plate\n")

    def test_a_missing_return_type_is_refused(self) -> None:
        with self.assertRaisesRegex(lab.WorkflowError, "no return type"):
            self._translate("def w(wf):\n    return None\n")

    def test_a_bare_expression_is_refused(self) -> None:
        with self.assertRaisesRegex(lab.WorkflowError, "wf.perform"):
            self._translate(
                "def w(wf) -> Material[Strain]:\n    detect_colonies(1)\n    return None\n"
            )


if __name__ == "__main__":
    unittest.main()
