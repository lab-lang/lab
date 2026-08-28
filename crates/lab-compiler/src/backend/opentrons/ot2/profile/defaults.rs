//! Reference-bench default values for every profile field.

use crate::backend::opentrons::ot2::profile::schema::{
    AssemblyStage, MediaRack, Pipette, Plates, PlatingStage, TemperatureModule, Thermocycler,
    TipRacks, TransformationStage,
};

pub(super) fn default_api_level() -> String {
    "2.21".to_owned()
}

pub(super) fn default_small_pipette() -> Pipette {
    Pipette {
        model: "p20_single_gen2".to_owned(),
        mount: "left".to_owned(),
    }
}

pub(super) fn default_large_pipette() -> Pipette {
    Pipette {
        model: "p300_single_gen2".to_owned(),
        mount: "right".to_owned(),
    }
}

pub(super) fn default_temperature_module() -> TemperatureModule {
    TemperatureModule {
        model: "temperature module gen2".to_owned(),
        slot: "1".to_owned(),
        labware: "opentrons_24_aluminumblock_nest_1.5ml_snapcap".to_owned(),
        capacity: 24,
    }
}

pub(super) fn default_thermocycler() -> Thermocycler {
    Thermocycler {
        model: "thermocycler module gen2".to_owned(),
        labware: "nest_96_wellplate_100ul_pcr_full_skirt".to_owned(),
        capacity: 96,
    }
}

pub(super) fn default_plate_capacity() -> usize {
    96
}

pub(super) fn default_assembly_small_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_tiprack_20ul".to_owned(),
        slots: vec!["2".to_owned()],
        capacity: default_plate_capacity(),
    }
}

pub(super) fn default_assembly_stage() -> AssemblyStage {
    AssemblyStage {
        small_tips: default_assembly_small_tips(),
    }
}

pub(super) fn default_transformation_dna_plate() -> Plates {
    Plates {
        labware: "nest_96_wellplate_100ul_pcr_full_skirt".to_owned(),
        slots: vec!["2".to_owned()],
        capacity: default_plate_capacity(),
    }
}

pub(super) fn default_transformation_small_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_tiprack_20ul".to_owned(),
        slots: vec!["3".to_owned()],
        capacity: default_plate_capacity(),
    }
}

pub(super) fn default_transformation_large_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_filtertiprack_200ul".to_owned(),
        slots: vec!["6".to_owned()],
        capacity: default_plate_capacity(),
    }
}

pub(super) fn default_transformation_stage() -> TransformationStage {
    TransformationStage {
        dna_plate: default_transformation_dna_plate(),
        small_tips: default_transformation_small_tips(),
        large_tips: default_transformation_large_tips(),
    }
}

pub(super) fn default_dilution_plate() -> Plates {
    Plates {
        labware: "nest_96_wellplate_100ul_pcr_full_skirt".to_owned(),
        slots: vec!["2".to_owned(), "3".to_owned()],
        capacity: default_plate_capacity(),
    }
}

pub(super) fn default_agar_plate() -> Plates {
    Plates {
        labware: "nest_96_wellplate_100ul_pcr_full_skirt".to_owned(),
        slots: vec!["5".to_owned(), "6".to_owned()],
        capacity: default_plate_capacity(),
    }
}

pub(super) fn default_media_rack() -> MediaRack {
    MediaRack {
        labware: "opentrons_15_tuberack_falcon_15ml_conical".to_owned(),
        slot: "4".to_owned(),
        medium_well: "A1".to_owned(),
    }
}

pub(super) fn default_plating_small_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_filtertiprack_20ul".to_owned(),
        slots: vec!["9".to_owned()],
        capacity: default_plate_capacity(),
    }
}

pub(super) fn default_plating_large_tips() -> TipRacks {
    TipRacks {
        labware: "opentrons_96_filtertiprack_200ul".to_owned(),
        slots: vec!["1".to_owned()],
        capacity: default_plate_capacity(),
    }
}

pub(super) fn default_plating_stage() -> PlatingStage {
    PlatingStage {
        dilution_plate: default_dilution_plate(),
        agar_plate: default_agar_plate(),
        media_rack: default_media_rack(),
        small_tips: default_plating_small_tips(),
        large_tips: default_plating_large_tips(),
    }
}
