from collections.abc import Callable

from opentrons import protocol_api

from lab_opentrons_ot2.protocols import assembly, plating, transformation


def test_protocol_entrypoints_are_importable_and_typed() -> None:
    protocols: list[tuple[dict[str, str], Callable[[protocol_api.ProtocolContext], None]]] = [
        (assembly.requirements, assembly.run),
        (transformation.requirements, transformation.run),
        (plating.requirements, plating.run),
    ]

    for requirements, run in protocols:
        assert requirements == {"robotType": "OT-2", "apiLevel": "2.21"}
        assert callable(run)
