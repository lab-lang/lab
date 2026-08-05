"""Static Python view of the OT-2 execution plan emitted by Rust."""

from typing import TypedDict

# LAB:BUNDLE_TYPES_START


class TransformationReaction(TypedDict):
    assembly_well: str
    culture_well: str


class PlatingLayout(TypedDict):
    culture_well: str
    dilution_wells: list[str]
    agar_wells: list[list[str]]


class AutomationConstruct(TypedDict):
    artifact: str
    sequence: str
    backbone: str
    components: list[str]
    steps: list[str]
    restriction_enzyme: str
    host: str
    selection: str
    assembly_replicates: int
    transformation_replicates: int
    plating_replicates: int
    serial_dilutions: int
    water_volume_ul: int
    assembly_wells: list[str]
    transformations: list[TransformationReaction]
    plating: list[PlatingLayout]


class Ot2ExecutionPlan(TypedDict):
    schema_version: str
    target: str
    api_level: str
    assembly_source_wells: dict[str, str]
    transformation_source_wells: dict[str, str]
    constructs: list[AutomationConstruct]


# LAB:BUNDLE_TYPES_END
