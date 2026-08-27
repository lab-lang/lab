"""Designs written through typed and raw pySBOL3 components.

The reporter example under `programs/reporter/plasmid.py` states its design
through `lab.sbol`; the hand-written Lab below is the same declaration with
the design's contribution spelled out. The two must compile to the same
checked module: what SBOL already states, the compiler reads. The focused edge
case tests below continue to pass ordinary `sbol3.Component` objects directly.
"""

import importlib.util
import re
import unittest
from typing import Any

import lab

HAVE_SBOL = bool(importlib.util.find_spec("sbol3"))
if HAVE_SBOL:
    import sbol3
    from lab import dna
    from lab.bio.designs import Plasmid
    from programs.reporter import plasmid as reporter_plasmid

HAND_WRITTEN = """\
/*!
 * The GFP reporter plasmid, designed through Lab's typed SBOL layer.
 */

use std.bio.designs
use std.bio.golden_gate

reporter_sequence: DNA = dna("ACGTACGT")

buy:
  promoter J23101:
    identity = "https://synbiohub.org/public/igem/BBa_J23101/1"

  part B0034:
    identity = "https://synbiohub.org/public/igem/BBa_B0034/1"

  cds GFP:
    identity = "https://synbiohub.org/public/igem/BBa_E0040/1"

  part B0015:
    identity = "https://synbiohub.org/public/igem/BBa_B0015/1"

  backbone pSB1C3:
    identity = "https://synbiohub.org/public/igem/pSB1C3/1"

  restriction_enzyme BsaI:
    identity = "NEB-R0535"
    digest_temperature = 37 C
    digest_duration = 2 min

/** The GFP reporter under a strong constitutive promoter. */
build plasmid reporter:
  components = [J23101, B0034, GFP, B0015]
  sequence = reporter_sequence
  backbone = pSB1C3
  restriction_enzyme = BsaI
  reaction_volume = 20 uL

  across 3 biological replicates

  require topology == circular

  accept concentration >= 100 ng/uL
  accept volume >= 20 uL across 1 biological replicate
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


@unittest.skipUnless(HAVE_SBOL, "sbol3 is required")
class SbolDesignTests(unittest.TestCase):
    def test_the_design_module_checks(self) -> None:
        program = lab.check(reporter_plasmid.module)

        self.assertIn("reporter.plasmid", program.checked)

    def test_the_design_matches_the_hand_written_lab(self) -> None:
        written = lab.check_sources({"reporter.plasmid": HAND_WRITTEN})
        emitted = lab.check(reporter_plasmid.module)

        self.assertEqual(
            normalize(emitted.checked["reporter.plasmid"]),
            normalize(written.checked["reporter.plasmid"]),
        )

    def test_components_follow_the_meets_constraints(self) -> None:
        # The features are handed over shuffled; the constraints state the
        # order, so the emitted list must follow them rather than the list.
        sbol3.set_namespace("https://example.org/shuffled")
        first = sbol3.SubComponent("https://example.org/parts/first/1")
        second = sbol3.SubComponent("https://example.org/parts/second/1")
        third = sbol3.SubComponent("https://example.org/parts/third/1")
        design = sbol3.Component(
            "shuffled",
            [sbol3.SBO_DNA],
            features=[third, first, second],
        )
        design.constraints = [
            sbol3.Constraint(sbol3.SBOL_MEETS, second, third),
            sbol3.Constraint(sbol3.SBOL_MEETS, first, second),
        ]

        module = lab.Module("shuffled.design")
        Plasmid.build(
            design=design,
            module=module,
            name="ordered",
            sequence=dna("ACGT"),
        )

        self.assertIn("components = [first, second, third]", module.source())

    def test_a_part_shared_by_two_designs_is_catalogued_once(self) -> None:
        sbol3.set_namespace("https://example.org/shared")
        module = lab.Module("shared.design")
        for name in ("one", "two"):
            part = sbol3.SubComponent("https://example.org/parts/common/1")
            design = sbol3.Component(name, [sbol3.SBO_DNA], features=[part])
            Plasmid.build(design=design, module=module, name=name, sequence=dna("ACGT"))

        source = module.source()
        self.assertEqual(source.count("part common"), 1)

    def test_a_component_that_is_not_dna_is_refused(self) -> None:
        sbol3.set_namespace("https://example.org/chemical")
        chemical = sbol3.Component("aTc", sbol3.SBO_SIMPLE_CHEMICAL)
        module = lab.Module("chemical.design")

        with self.assertRaisesRegex(lab.DesignError, "must be DNA"):
            Plasmid.build(design=chemical, module=module, name="wrong")

    def test_a_raw_sequence_becomes_one_named_lab_value(self) -> None:
        sbol3.set_namespace("https://example.org/raw-sequence")
        document = sbol3.Document()
        sequence = sbol3.Sequence(
            "reporter_sequence",
            elements="ACGT",
            encoding=sbol3.IUPAC_DNA_ENCODING,
        )
        design = sbol3.Component("reporter", sbol3.SBO_DNA, sequences=[sequence])
        document.add([sequence, design])
        module = lab.Module("raw.sequence")
        Plasmid.build(design=design, module=module, name="reporter")

        source = module.source()

        self.assertIn('reporter_sequence: DNA = dna("ACGT")', source)
        self.assertIn("sequence = reporter_sequence", source)
        self.assertEqual(source.count('DNA = dna("ACGT")'), 1)

    def test_a_broken_meets_chain_is_refused(self) -> None:
        sbol3.set_namespace("https://example.org/broken")
        first = sbol3.SubComponent("https://example.org/parts/first/1")
        second = sbol3.SubComponent("https://example.org/parts/second/1")
        third = sbol3.SubComponent("https://example.org/parts/third/1")
        design = sbol3.Component("broken", [sbol3.SBO_DNA], features=[first, second, third])
        design.constraints = [sbol3.Constraint(sbol3.SBOL_MEETS, first, second)]

        module = lab.Module("broken.design")
        Plasmid.build(design=design, module=module, name="wrong")
        with self.assertRaisesRegex(lab.DesignError, "meets"):
            module.source()


if __name__ == "__main__":
    unittest.main()
