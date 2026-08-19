"""Networks written as LOICA genetic networks.

A LOICA network is a set of transcription units wired by the gene products
they express, so it lowers to one Lab circuit per unit bound into a list. The
reporter example under `programs/reporter/` holds the single-unit form and the
repressilator holds the three-unit ring; the hand-written Lab below is the
same ring in Lab's own syntax, and the two must compile to the same checked
module.

The network is read structurally, so one class of tests runs against the real
LOICA library and another against plain objects with a network's shape, which
is what lets the SDK work without LOICA installed.
"""

import importlib.util
import re
import unittest
from typing import Any

import lab

HAVE_BIO = bool(importlib.util.find_spec("sbol3")) and bool(importlib.util.find_spec("loica"))
if HAVE_BIO:
    from programs.reporter import circuit as reporter_circuit
    from programs.reporter import repressilator as reporter_ring

RING = """\
/*!
 * The repressilator, written as a LOICA genetic network.
 *
 * Three transcription units in a ring, each repressor shutting off the next.
 * Nothing induces it from outside and nothing reports out of it: the whole
 * network is regulators wired to each other, which is what makes it the
 * sharpest test that the wiring is carried by the types.
 */

use std.bio.designs
use std.bio.parts

record TetR is Protein, Signal

record LacI is Protein, Signal

record CI is Protein, Signal

buy:
  promoter pLac: Promoter<LacI>:
    identity = "https://example.org/repressilator/pLac"
    regulation = repressed

  cds TetR_cds: CDS<TetR>:
    identity = "https://example.org/repressilator/TetR"

  promoter pTet_promoter: Promoter<TetR>:
    identity = "https://example.org/repressilator/pTet"
    regulation = repressed

  cds CI_cds: CDS<CI>:
    identity = "https://example.org/repressilator/CI"

  promoter pCI: Promoter<CI>:
    identity = "https://example.org/repressilator/pCI"
    regulation = repressed

  cds LacI_cds: CDS<LacI>:
    identity = "https://example.org/repressilator/LacI"

/**
 * One transcription unit: a promoter driving a coding sequence
 * through the shared RBS and terminator.
 */
circuit repressilator_unit(
  promoter: Promoter<Trigger: Signal>,
  coding: CDS<Product: Protein>,
) -> Circuit<Trigger, Product>:
  layout:
    promoter
    B0034
    coding
    B0015

repressilator_1 = repressilator_unit(pLac, TetR_cds)

repressilator_2 = repressilator_unit(pTet_promoter, CI_cds)

repressilator_3 = repressilator_unit(pCI, LacI_cds)

/**
 * Three repressors in a ring, each shutting off the next.
 */
ring = [repressilator_1, repressilator_2, repressilator_3]
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


def bindings(checked: dict[str, Any]) -> dict[str, Any]:
    """Every binding in a checked module, by the name it binds."""

    found = {}
    for declaration in checked["declarations"]:
        if declaration["kind"] == "binding":
            for target in declaration["targets"]:
                found[target["name"]] = target["type"]
    return found


def arguments(ty: dict[str, Any]) -> list[str]:
    return [argument["name"] for argument in ty["arguments"]]


@unittest.skipUnless(HAVE_BIO, "sbol3 and loica are required")
class SingleUnitTests(unittest.TestCase):
    def test_the_circuit_module_checks(self) -> None:
        program = lab.check(reporter_circuit.module)

        self.assertIn("reporter.circuit", program.checked)

    def test_the_unit_reads_trigger_and_product_off_the_network(self) -> None:
        program = lab.check(reporter_circuit.module)

        bound = bindings(program.checked["reporter.circuit"])
        self.assertEqual(arguments(bound["regulated_expression_1"]), ["ATc", "SfGFP"])

    def test_a_one_unit_network_is_still_a_list(self) -> None:
        program = lab.check(reporter_circuit.module)

        bound = bindings(program.checked["reporter.circuit"])
        self.assertEqual(bound["tet_reporter"]["kind"], "list")

    def test_a_receiver_is_induced_by_its_supplement(self) -> None:
        source = reporter_circuit.module.source()

        self.assertIn("regulation = induced", source)

    def test_characterization_rides_on_each_unit(self) -> None:
        self.assertEqual(
            reporter_circuit.tet_reporter.characterization,
            [{"alpha": [0, 100], "K": 1, "n": 2}],
        )


@unittest.skipUnless(HAVE_BIO, "sbol3 and loica are required")
class RepressilatorTests(unittest.TestCase):
    """Three units in a ring: the case a single-unit lowering cannot state."""

    def test_the_ring_checks(self) -> None:
        program = lab.check(reporter_ring.module)

        self.assertIn("reporter.repressilator", program.checked)

    def test_the_ring_matches_the_hand_written_lab(self) -> None:
        written = lab.check_sources({"reporter.repressilator": RING})
        emitted = lab.check(reporter_ring.module)

        self.assertEqual(
            normalize(emitted.checked["reporter.repressilator"]),
            normalize(written.checked["reporter.repressilator"]),
        )

    def test_each_unit_is_typed_by_the_regulator_it_answers_to(self) -> None:
        program = lab.check(reporter_ring.module)

        bound = bindings(program.checked["reporter.repressilator"])
        self.assertEqual(arguments(bound["repressilator_1"]), ["LacI", "TetR"])
        self.assertEqual(arguments(bound["repressilator_2"]), ["TetR", "CI"])
        self.assertEqual(arguments(bound["repressilator_3"]), ["CI", "LacI"])

    def test_a_regulator_is_both_a_protein_and_a_signal(self) -> None:
        program = lab.check(reporter_ring.module)

        checked = program.checked["reporter.repressilator"]
        roles = {
            declaration["name"]: declaration["roles"]
            for declaration in checked["declarations"]
            if declaration["kind"] == "data"
        }
        for regulator in ("TetR", "LacI", "CI"):
            with self.subTest(regulator=regulator):
                self.assertEqual(sorted(roles[regulator]), ["Protein", "Signal"])

    def test_repression_is_read_from_the_hill_parameters(self) -> None:
        source = reporter_ring.module.source()

        self.assertEqual(source.count("regulation = repressed"), 3)
        self.assertNotIn("regulation = induced", source)

    def test_the_network_binds_every_unit(self) -> None:
        self.assertEqual(
            [unit.name for unit in reporter_ring.ring.units],
            ["repressilator_1", "repressilator_2", "repressilator_3"],
        )


class _Component:
    """The shape of an sbol3.Component, without sbol3."""

    def __init__(self, display_id: str, roles: list[str] | None = None) -> None:
        self.display_id = display_id
        self.identity = f"https://example.org/parts/{display_id}"
        self.types = ["https://identifiers.org/SBO:0000251"]
        self.roles = roles or []


class _Part:
    def __init__(self, name: str) -> None:
        self.name = name
        self.sbol_comp = _Component(name)


class _Operator:
    def __init__(self, input: object, output: object, alpha: list[float] | None = None) -> None:
        self.input = input
        self.output = output
        self.alpha = alpha
        self.sbol_comp = None


class _Network:
    def __init__(self, operators: list[object]) -> None:
        self.operators = operators


def _lower(network: _Network, name: str = "shape") -> lab.NetworkBinding:
    from lab.bio.parts import B0015, B0034

    module = lab.Module(f"{name}.circuit")

    @lab.circuit
    def built() -> lab.Layout:
        return lab.layout(network, rbs=B0034, terminator=B0015)

    return built(name="network", module=module)


class StructuralNetworkTests(unittest.TestCase):
    """The network is duck-typed, so LOICA's shape is enough without LOICA."""

    def test_a_cascade_wires_one_units_product_to_the_next_units_trigger(self) -> None:
        aTc, tetR, gfp = _Part("aTc"), _Part("TetR"), _Part("Gfp")
        network = _Network([_Operator(aTc, tetR, [0, 100]), _Operator(tetR, gfp, [0, 100])])

        binding = _lower(network, "cascade")

        program = lab.check(binding.module)
        bound = bindings(program.checked["cascade.circuit"])
        self.assertEqual(arguments(bound["built_1"]), ["ATc", "TetR"])
        self.assertEqual(arguments(bound["built_2"]), ["TetR", "Gfp"])

    def test_a_constitutive_source_answers_to_a_named_condition(self) -> None:
        network = _Network([_Operator(None, _Part("Gfp"))])

        binding = _lower(network, "constitutive")

        source = binding.module.source()
        self.assertIn("record Constitutive is Signal", source)
        program = lab.check(binding.module)
        bound = bindings(program.checked["constitutive.circuit"])
        self.assertEqual(arguments(bound["built_1"]), ["Constitutive", "Gfp"])

    def test_a_two_input_operator_combines_its_signals(self) -> None:
        tetR, lacI, gfp = _Part("TetR"), _Part("LacI"), _Part("Gfp")
        network = _Network([_Operator([tetR, lacI], gfp, [0, 10, 10, 100])])

        binding = _lower(network, "gate")

        self.assertIn("Promoter<Both<TetR, LacI>>", binding.module.source())
        lab.check(binding.module)

    def test_a_three_input_operator_nests_its_signals(self) -> None:
        parts = [_Part("TetR"), _Part("LacI"), _Part("AraC")]
        network = _Network([_Operator(parts, _Part("Gfp"), [0, 100])])

        binding = _lower(network, "three")

        self.assertIn("Promoter<Both<TetR, Both<LacI, AraC>>>", binding.module.source())
        lab.check(binding.module)

    def test_a_polycistronic_operator_combines_its_products(self) -> None:
        network = _Network([_Operator(_Part("aTc"), [_Part("Gfp"), _Part("Rfp")], [0, 100])])

        binding = _lower(network, "poly")

        self.assertIn("CDS<Operon<Gfp, Rfp>>", binding.module.source())
        lab.check(binding.module)

    def test_fan_out_reuses_one_regulator_across_units(self) -> None:
        tetR = _Part("TetR")
        network = _Network(
            [
                _Operator(_Part("aTc"), tetR, [0, 100]),
                _Operator(tetR, _Part("Gfp"), [0, 100]),
                _Operator(tetR, _Part("Rfp"), [0, 100]),
            ]
        )

        binding = _lower(network, "fanout")

        source = binding.module.source()
        self.assertEqual(source.count("record TetR is Protein, Signal"), 1)
        lab.check(binding.module)

    def test_two_parts_sharing_a_name_stay_two_parts(self) -> None:
        network = _Network([_Operator(_Part("X"), _Part("X"), [0, 100])])

        binding = _lower(network, "samename")

        lab.check(binding.module)

    def test_an_empty_network_is_refused(self) -> None:
        with self.assertRaisesRegex(lab.CircuitError, "no operators"):
            _lower(_Network([]), "empty")

    def test_an_operator_expressing_nothing_is_refused(self) -> None:
        with self.assertRaisesRegex(lab.CircuitError, "expresses nothing"):
            _lower(_Network([_Operator(_Part("aTc"), None)]), "noproduct")

    def test_a_bare_network_return_is_refused(self) -> None:
        module = lab.Module("shape.bare")

        @lab.circuit
        def bare() -> lab.Layout:
            return _Network([])  # type: ignore[return-value]

        with self.assertRaisesRegex(lab.CircuitError, "lab.layout"):
            bare(name="x", module=module)


if __name__ == "__main__":
    unittest.main()
