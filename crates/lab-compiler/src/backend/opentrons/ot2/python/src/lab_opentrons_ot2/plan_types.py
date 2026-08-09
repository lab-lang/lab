"""Static Python view of the OT-2 execution plan emitted by Rust."""

from typing import TypedDict

# LAB:BUNDLE_TYPES_START


class Pipette(TypedDict):
    model: str
    mount: str


class Instruments(TypedDict):
    small: Pipette
    large: Pipette


class TemperatureModule(TypedDict):
    model: str
    slot: str
    labware: str
    capacity: int


class Thermocycler(TypedDict):
    model: str
    labware: str
    capacity: int


class SharedDeck(TypedDict):
    temperature_module: TemperatureModule
    thermocycler: Thermocycler


class Plates(TypedDict):
    labware: str
    slots: list[str]
    capacity: int


class MediaRack(TypedDict):
    labware: str
    slot: str
    medium_well: str


class AssemblyStage(TypedDict):
    small_tips: Plates


class TransformationStage(TypedDict):
    dna_plate: Plates
    small_tips: Plates
    large_tips: Plates


class PlatingStage(TypedDict):
    dilution_plate: Plates
    agar_plate: Plates
    media_rack: MediaRack
    small_tips: Plates
    large_tips: Plates


class Stages(TypedDict):
    assembly: AssemblyStage
    transformation: TransformationStage
    plating: PlatingStage


class TargetMetadata(TypedDict):
    name: str
    backend: str
    api_level: str


class TargetProfile(TypedDict):
    target: TargetMetadata
    instruments: Instruments
    deck: SharedDeck
    stages: Stages


class Well(TypedDict):
    plate: int
    well: str


class TransformationReaction(TypedDict):
    culture_well: str
    source_wells: list[Well]


class PlatingLayout(TypedDict):
    culture_well: str
    dilution_wells: list[Well]
    agar_wells: list[list[Well]]


class AssemblyChemistry(TypedDict):
    reaction_volume_ul: int
    part_volume_ul: int
    enzyme_volume_ul: int
    ligase_volume_ul: int
    buffer_volume_ul: int
    cycles: int
    digest_temperature_c: int
    digest_minutes: int
    ligate_temperature_c: int
    ligate_minutes: int


class StrainChemistry(TypedDict):
    cell_volume_ul: int
    dna_volume_ul: int
    recovery_volume_ul: int
    cold_minutes: int
    heat_shock_temperature_c: int
    heat_shock_minutes: int
    recovery_temperature_c: int
    recovery_minutes: int
    medium_volume_ul: int
    culture_volume_ul: int
    colony_volume_ul: int


class AssemblyPlan(TypedDict):
    artifact: str
    sequence: str
    backbone: str
    components: list[str]
    dependencies: list[str]
    restriction_enzyme: str
    assembly_replicates: int
    water_volume_ul: int
    assembly_wells: list[str]
    chemistry: AssemblyChemistry


class StrainPlan(TypedDict):
    artifact: str
    host: str
    plasmids: list[str]
    dependencies: list[str]
    selection: str
    transformation_replicates: int
    plating_replicates: int
    serial_dilutions: int
    transformations: list[TransformationReaction]
    plating: list[PlatingLayout]
    chemistry: StrainChemistry


class Ot2ExecutionPlan(TypedDict):
    schema_version: str
    target: str
    api_level: str
    deck: TargetProfile
    assembly_source_wells: dict[str, str]
    transformation_source_wells: dict[str, str]
    dna_source_wells: dict[str, Well]
    assemblies: list[AssemblyPlan]
    strains: list[StrainPlan]


# LAB:BUNDLE_TYPES_END
