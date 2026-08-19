"""Circuits written as LOICA genetic networks.

The reporter example under `programs/reporter/circuit.py` is the LOICA form
of `regulated_expression`; the hand-written Lab below is the same circuit in
Lab's own syntax. The two must compile to the same checked module.

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

HAND_WRITTEN = """\
/*!
 * The tet reporter circuit, written as a LOICA genetic network.
 */

use std.bio.designs
use std.bio.parts

record ATc is Signal

record SfGFP is Protein

buy:
  promoter pTet_promoter: Promoter<ATc>:
    identity = "https://synbiohub.org/user/marpaia/reporter/pTet"
  cds sfGFP_cds: CDS<SfGFP>:
    identity = "https://synbiohub.org/user/marpaia/reporter/sfGFP"

/** A promoter driving a coding sequence through a shared RBS and terminator. */
circuit regulated_expression(
  promoter: Promoter<Trigger: Signal>,
  coding: CDS<Product: Protein>,
) -> Circuit<Trigger, Product>:
  layout:
    promoter
    B0034
    coding
    B0015

tet_reporter = regulated_expression(pTet_promoter, sfGFP_cds)
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


@unittest.skipUnless(HAVE_BIO, "sbol3 and loica are required")
class LoicaCircuitTests(unittest.TestCase):
    def test_the_circuit_module_checks(self) -> None:
        program = lab.check(reporter_circuit.module)

        self.assertIn("reporter.circuit", program.checked)

    def test_the_binding_reads_trigger_and_product_off_the_network(self) -> None:
        program = lab.check(reporter_circuit.module)

        checked = program.checked["reporter.circuit"]
        binding = next(d for d in checked["declarations"] if d["kind"] == "binding")
        target = binding["targets"][0]
        self.assertEqual(target["name"], "tet_reporter")
        self.assertEqual(target["type"]["name"], "Circuit")
        self.assertEqual(
            [argument["name"] for argument in target["type"]["arguments"]],
            ["ATc", "SfGFP"],
        )

    def test_the_network_matches_the_hand_written_lab(self) -> None:
        written = lab.check_sources({"reporter.circuit": HAND_WRITTEN})
        emitted = lab.check(reporter_circuit.module)

        self.assertEqual(
            normalize(emitted.checked["reporter.circuit"]),
            normalize(written.checked["reporter.circuit"]),
        )

    def test_parts_carry_their_sbol_identities(self) -> None:
        program = lab.check(reporter_circuit.module)

        checked = program.checked["reporter.circuit"]
        identities = {
            d["name"]: d["identity"] for d in checked["declarations"] if d["kind"] == "catalog"
        }
        self.assertEqual(
            identities,
            {
                "pTet_promoter": "https://synbiohub.org/user/marpaia/reporter/pTet",
                "sfGFP_cds": "https://synbiohub.org/user/marpaia/reporter/sfGFP",
            },
        )

    def test_characterization_rides_on_the_binding(self) -> None:
        self.assertEqual(
            reporter_circuit.tet_reporter.characterization,
            {"alpha": [0, 100], "K": 1, "n": 2},
        )


class _Component:
    """The shape of an sbol3.Component, without sbol3."""

    def __init__(self, display_id: str, types: list[str], roles: list[str] | None = None) -> None:
        self.display_id = display_id
        self.identity = f"https://example.org/parts/{display_id}"
        self.types = types
        self.roles = roles or []


class _Part:
    def __init__(self, name: str, sbol_comp: _Component | None = None) -> None:
        self.name = name
        self.sbol_comp = sbol_comp


class _Operator:
    def __init__(self, input: object, output: object, sbol_comp: _Component | None) -> None:
        self.input = input
        self.output = output
        self.sbol_comp = sbol_comp


class _Network:
    def __init__(self, operators: list[object]) -> None:
        self.operators = operators


def _network() -> _Network:
    inducer = _Part("iptg", _Component("iptg", ["https://identifiers.org/SBO:0000247"]))
    product = _Part(
        "mCherry",
        _Component(
            "mCherry",
            ["https://identifiers.org/SBO:0000251"],
            ["https://identifiers.org/SO:0000316"],
        ),
    )
    operator = _Operator(
        inducer,
        product,
        _Component(
            "pLac",
            ["https://identifiers.org/SBO:0000251"],
            ["https://identifiers.org/SO:0000167"],
        ),
    )
    return _Network([operator])


class StructuralNetworkTests(unittest.TestCase):
    """The network is duck-typed, so LOICA's shape is enough without LOICA."""

    def _instantiate(self, network: _Network) -> lab.CircuitBinding:
        from lab.bio.parts import B0015, B0034

        module = lab.Module("shape.circuit")

        @lab.circuit
        def lac_expression() -> lab.Layout:
            return lab.layout(network, rbs=B0034, terminator=B0015)

        return lac_expression(name="lac_reporter", module=module)

    def test_a_network_shaped_object_lowers_without_loica(self) -> None:
        binding = self._instantiate(_network())

        program = lab.check(binding.module)
        checked = program.checked["shape.circuit"]
        target = next(d for d in checked["declarations"] if d["kind"] == "binding")
        self.assertEqual(
            [argument["name"] for argument in target["targets"][0]["type"]["arguments"]],
            ["Iptg", "MCherry"],
        )

    def test_a_network_with_two_operators_is_refused(self) -> None:
        network = _network()
        network.operators = network.operators * 2

        with self.assertRaisesRegex(lab.CircuitError, "exactly one"):
            self._instantiate(network)

    def test_an_operator_without_an_input_is_refused(self) -> None:
        network = _network()
        network.operators = [_Operator(None, _Part("gfp"), None)]

        with self.assertRaisesRegex(lab.CircuitError, "no input"):
            self._instantiate(network)

    def test_a_mistyped_inducer_component_is_refused(self) -> None:
        network = _network()
        inducer = _Part("iptg", _Component("iptg", ["https://identifiers.org/SBO:0000251"]))
        network.operators = [_Operator(inducer, _Part("mCherry"), None)]

        with self.assertRaisesRegex(lab.CircuitError, "SBO:0000247"):
            self._instantiate(network)

    def test_a_bare_network_return_is_refused(self) -> None:
        module = lab.Module("shape.bare")

        @lab.circuit
        def bare() -> lab.Layout:
            return _network()  # type: ignore[return-value]

        with self.assertRaisesRegex(lab.CircuitError, "lab.layout"):
            bare(name="x", module=module)


if __name__ == "__main__":
    unittest.main()
