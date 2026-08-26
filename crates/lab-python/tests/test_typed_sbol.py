"""Typed SBOL construction without direct pySBOL3 graph manipulation."""

import unittest
from typing import TYPE_CHECKING, Any, assert_type, cast

import lab
from lab import sbol
from lab.bio.designs import CDS as LabCds
from lab.bio.designs import Backbone as LabBackbone
from lab.bio.designs import Part as LabPart
from lab.bio.designs import Promoter as LabPromoter
from lab.bio.golden_gate import Plasmid as LabPlasmid

if TYPE_CHECKING:
    type_document = sbol.Document(namespace="https://example.org/typecheck")
    type_promoter = type_document.promoter(identity="promoter")
    type_cds = type_document.cds(identity="cds")
    type_plasmid = type_document.plasmid(components=[type_promoter, type_cds])
    type_module = lab.Module("typecheck.designs")

    assert_type(type_promoter, sbol.PromoterDesign)
    assert_type(type_cds, sbol.CodingSequence)
    assert_type(type_plasmid, sbol.PlasmidDesign)
    assert_type(
        LabBackbone.buy(module=type_module, name="backbone"),
        lab.BuyDeclaration[LabBackbone],
    )
    assert_type(
        LabPlasmid.build(design=type_plasmid, module=type_module, name="plasmid"),
        lab.BuildDeclaration[LabPlasmid],
    )


def reporter_document(module: lab.Module | None = None) -> tuple[sbol.Document, sbol.Plasmid]:
    document = sbol.Document(namespace="https://example.org/reporter")
    promoter = document.promoter(identity="J23101", sequence="TTGACA")
    rbs = document.rbs(identity="B0034", sequence="AAAGAGGAGA")
    coding = document.cds(identity="GFP", sequence="ATGACCTAA")
    terminator = document.terminator(identity="B0015", sequence="CCAGGC")
    components: list[sbol.DnaComponentInput]
    if module is None:
        components = [promoter, rbs, coding, terminator]
    else:
        components = [
            LabPromoter.buy(design=promoter, module=module, name="J23101"),
            LabPart.buy(design=rbs, module=module, name="B0034"),
            LabCds.buy(design=coding, module=module, name="GFP"),
            LabPart.buy(design=terminator, module=module, name="B0015"),
        ]
    plasmid = document.plasmid(
        components=components,
        sequence="TTGACAAAAGAGGAGAATGACCTAAC CAGGC".replace(" ", ""),
        description="The GFP reporter under a constitutive promoter.",
    )
    return document, plasmid


class TypedSbolTests(unittest.TestCase):
    def test_public_design_inputs_are_keyword_only(self) -> None:
        with self.assertRaises(TypeError):
            cast(Any, sbol.Document)("https://example.org/positional")

        document = sbol.Document(namespace="https://example.org/positional")
        with self.assertRaises(TypeError):
            cast(Any, document.promoter)("promoter")
        with self.assertRaises(TypeError):
            cast(Any, document.plasmid)([])

    def test_factories_preserve_biological_types_and_sequences(self) -> None:
        document, plasmid = reporter_document()

        self.assertIsInstance(plasmid, sbol.Plasmid)
        self.assertIsInstance(plasmid.components[0], sbol.Promoter)
        self.assertIsInstance(plasmid.components[1], sbol.RibosomeBindingSite)
        self.assertIsInstance(plasmid.components[2], sbol.CodingSequence)
        self.assertIsInstance(plasmid.components[3], sbol.Terminator)
        self.assertIsNotNone(plasmid.sequence)
        assert plasmid.sequence is not None
        self.assertEqual(plasmid.sequence.elements, "TTGACAAAAGAGGAGAATGACCTAACCAGGC")
        self.assertIs(plasmid.topology, sbol.Topology.CIRCULAR)
        self.assertEqual(len(document.components), 5)
        self.assertIsNone(plasmid.identity)
        with self.assertRaisesRegex(sbol.SbolIdentityError, "anonymous design"):
            _ = document.sbol3_document

    def test_typed_plasmid_lowers_through_the_existing_lab_boundary(self) -> None:
        module = lab.Module("reporter.typed_design")
        document, plasmid = reporter_document(module)

        declaration = LabPlasmid.build(design=plasmid, module=module, name="reporter")
        source = module.source()

        self.assertIs(declaration.kind, LabPlasmid)
        self.assertIsInstance(declaration, lab.BuildDeclaration)
        self.assertEqual(plasmid.identity, "https://example.org/reporter/reporter")
        self.assertIn("components = [J23101, B0034, GFP, B0015]", source)
        self.assertIn("promoter J23101", source)
        self.assertIn("cds GFP", source)
        self.assertIn('sequence = dna("TTGACAAAAGAGGAGAATGACCTAACCAGGC")', source)
        self.assertIn("require topology == circular", source)
        lab.check(module)
        document.validate()

    def test_typed_components_require_explicit_build_or_buy_provenance(self) -> None:
        _, plasmid = reporter_document()
        module = lab.Module("reporter.unsourced")
        LabPlasmid.build(design=plasmid, module=module, name="reporter")

        with self.assertRaisesRegex(lab.DesignError, "no Lab provenance"):
            module.source()

    def test_a_bought_design_contributes_its_registry_identity(self) -> None:
        identity = "https://synbiohub.org/public/igem/pSB1C3/1"
        document = sbol.Document(namespace="https://example.org/purchases")
        design = document.backbone(identity=identity)
        module = lab.Module("purchases.backbones")

        declaration = LabBackbone.buy(design=design, module=module, name="pSB1C3")
        source = module.source()

        self.assertIsInstance(declaration, lab.BuyDeclaration)
        self.assertIn(f'identity = "{identity}"', source)
        self.assertNotIn("require topology", source)

    def test_module_emission_materializes_and_validates_only_once(self) -> None:
        module = lab.Module("reporter.repeatable")
        document, plasmid = reporter_document(module)
        LabPlasmid.build(design=plasmid, module=module, name="reporter")

        first = module.source()
        raw = cast(Any, document.sbol3_document)
        object_count = len(raw.objects)
        second = module.source()

        self.assertEqual(first, second)
        self.assertEqual(len(raw.objects), object_count)

    def test_module_emission_reports_invalid_generated_sbol(self) -> None:
        document = sbol.Document(namespace="https://example.org/invalid")
        design = document.plasmid(identity="reporter", sequence="ACGT")
        raw = cast(Any, design.sbol3_component)
        raw.types = []
        module = lab.Module("invalid.sbol")
        LabPlasmid.build(design=design, module=module, name="reporter")

        with self.assertRaisesRegex(sbol.SbolValidationError, "Too few values"):
            module.source()

    def test_anonymous_design_reuse_requires_an_explicit_identity(self) -> None:
        document = sbol.Document(namespace="https://example.org/reuse")
        design = document.plasmid(sequence="ACGT")
        first_module = lab.Module("reuse.first")
        second_module = lab.Module("reuse.second")
        LabPlasmid.build(design=design, module=first_module, name="first")
        LabPlasmid.buy(design=design, module=second_module, name="second")

        first_module.source()
        with self.assertRaisesRegex(sbol.SbolIdentityError, "explicit identity"):
            second_module.source()

    def test_a_non_dna_component_cannot_enter_a_plasmid_layout(self) -> None:
        document = sbol.Document(namespace="https://example.org/wrong-kind")
        protein = document.protein(identity="GFP", sequence="MSKGE")

        with self.assertRaisesRegex(TypeError, "not a DNA design"):
            document.plasmid(components=cast(Any, [protein]))

    def test_a_typed_part_cannot_masquerade_as_a_plasmid_design(self) -> None:
        document = sbol.Document(namespace="https://example.org/wrong-builder")
        promoter = document.promoter(identity="promoter", sequence="ACGT")
        module = lab.Module("wrong.builder")

        with self.assertRaisesRegex(lab.DesignError, "promoter cannot be passed"):
            LabPlasmid.build(design=promoter, module=module, name="wrong")

    def test_backbone_and_plasmid_remain_distinct_types(self) -> None:
        document = sbol.Document(namespace="https://example.org/kinds")
        backbone = document.backbone(identity="pSB1C3", sequence="ACGT")
        plasmid = document.plasmid(identity="reporter")

        self.assertIsInstance(backbone, sbol.Backbone)
        self.assertNotIsInstance(backbone, sbol.Plasmid)
        self.assertIsInstance(plasmid, sbol.Plasmid)
        self.assertNotIsInstance(plasmid, sbol.Backbone)

    def test_duplicate_identities_are_refused_before_document_mutation(self) -> None:
        document = sbol.Document(namespace="https://example.org/duplicates")
        document.promoter(identity="part")

        with self.assertRaisesRegex(ValueError, "already in this document"):
            document.terminator(identity="part")

    def test_a_composite_cannot_mix_documents(self) -> None:
        first = sbol.Document(namespace="https://example.org/first")
        second = sbol.Document(namespace="https://example.org/second")
        promoter = first.promoter(identity="promoter")

        with self.assertRaisesRegex(ValueError, "another SBOL document"):
            second.plasmid(components=[promoter])

    def test_registry_identity_is_a_typed_reference_not_a_copied_top_level(self) -> None:
        document = sbol.Document(namespace="https://example.org/local")
        promoter = document.promoter(identity="https://synbiohub.org/public/igem/BBa_J23101/1")

        self.assertIsInstance(promoter, sbol.Promoter)
        self.assertTrue(promoter.is_reference)
        self.assertIsNone(promoter.sbol3_component)
        document.validate()

    def test_one_registry_identity_cannot_change_kind_between_designs(self) -> None:
        identity = "https://synbiohub.org/public/example/shared/1"
        promoter_document = sbol.Document(namespace="https://example.org/promoter")
        promoter = promoter_document.promoter(identity=identity)
        coding_document = sbol.Document(namespace="https://example.org/coding")
        coding = coding_document.cds(identity=identity)
        module = lab.Module("conflicting.designs")
        bought_promoter = LabPromoter.buy(design=promoter, module=module, name="shared_promoter")
        first = promoter_document.plasmid(identity="first", components=[bought_promoter])
        bought_coding = LabCds.buy(design=coding, module=module, name="shared_cds")
        second = coding_document.plasmid(identity="second", components=[bought_coding])

        LabPlasmid.build(design=first, module=module, name="first")
        LabPlasmid.build(design=second, module=module, name="second")
        with self.assertRaisesRegex(lab.DesignError, "used as both Promoter and CDS"):
            module.source()

    def test_vocabulary_classes_with_the_same_kind_share_one_semantic_kind(self) -> None:
        identity = "https://synbiohub.org/public/example/reporter/1"
        first_document = sbol.Document(namespace="https://example.org/first")
        first = first_document.plasmid(identity=identity)
        second_document = sbol.Document(namespace="https://example.org/second")
        second = second_document.plasmid(identity=identity)
        module = lab.Module("compatible.plasmids")

        LabPlasmid.buy(design=first, module=module, name="first")
        LabPlasmid.buy(design=second, module=module, name="second")

        source = module.source()
        self.assertEqual(source.count(f'identity = "{identity}"'), 2)

    def test_raw_py_sbol_objects_remain_an_explicit_escape_hatch(self) -> None:
        module = lab.Module("raw.escape")
        document, plasmid = reporter_document(module)
        LabPlasmid.build(design=plasmid, module=module, name="reporter")
        module.source()

        self.assertIs(plasmid.sbol3_component, plasmid.__lab_sbol_component__())
        self.assertIsNotNone(document.sbol3_document)
