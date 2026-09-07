"""Python discovers and validates the compiler's exact adapter contracts."""

import lab
import pytest


def test_catalog_exposes_semantic_support_separately_from_features() -> None:
    catalog = lab.adapter_catalog()
    ot2 = catalog.get("opentrons.ot2")

    assert catalog.format == "lab.adapter-catalog.v4"
    assert "python-protocol-api" in ot2.features
    assert ot2.default_profile.driver == ot2.id
    assert len(ot2.procedure_implementations) == 2
    pipetting = next(
        i for i in ot2.procedure_implementations if i.contract.endswith("#PipettingProgramV1")
    )
    assert pipetting.services.planning
    assert pipetting.services.lowering
    assert not pipetting.services.runtime
    assert "multi_position_vessel" in pipetting.program_features
    assert "aspirate_tracked_surface" in pipetting.program_features
    assert "https://sbol.io/ns/capability#MeteredLiquidTransfer" in pipetting.capability_kinds
    # Support is contract-wide. Biological operation names are not adapter routing keys.
    assert not hasattr(pipetting, "operations")
    thermal = next(
        i for i in ot2.procedure_implementations if i.contract.endswith("#ThermalProgramV1")
    )
    assert (
        "https://sbol.io/ns/capability#ProgrammedBlockTemperatureControl"
        in thermal.capability_kinds
    )


def test_profile_validation_uses_the_explicit_driver() -> None:
    profile = lab.validate_adapter_profile(
        "opentrons.ot2",
        '[protocol]\napi_level = "2.22"\n',
        name="caltech-ot2",
    )

    assert profile.name == "caltech-ot2"
    assert profile.driver == "opentrons.ot2"
    assert profile.canonical_json["protocol"]["api_level"] == "2.22"
    assert len(profile.sha256) == 64

    with pytest.raises(ValueError, match="unknown field `target`"):
        lab.validate_adapter_profile("opentrons.ot2", 'target = "opentrons.flex"\n')


def test_catalog_lookup_never_uses_a_manufacturer_alias() -> None:
    with pytest.raises(KeyError):
        lab.adapter_catalog().get("Opentrons")
