//! Named reaction parameters shared by the Workflow and Protocol dialects.
//!
//! Chemistry is scientific intent, so it is preserved verbatim from source
//! through both dialects and only interpreted when a backend renders a
//! procedure. Keeping the key sets here means one list defines what a recipe
//! contains, what verification demands, and what emission may read.

/// Golden Gate reaction parameters carried by an assembly.
pub(crate) const ASSEMBLY_CHEMISTRY_KEYS: &[&str] = &[
    "reaction_volume_ul",
    "part_volume_ul",
    "enzyme_volume_ul",
    "ligase_volume_ul",
    "buffer_volume_ul",
    "cycles",
    "digest_temperature_c",
    "digest_minutes",
    "ligate_temperature_c",
    "ligate_minutes",
    "lid_temperature_c",
    "final_digest_temperature_c",
    "final_digest_minutes",
    "heat_inactivation_temperature_c",
    "heat_inactivation_minutes",
    "hold_temperature_c",
];

/// Heat-shock transformation and plating parameters carried by a strain.
pub(crate) const STRAIN_CHEMISTRY_KEYS: &[&str] = &[
    "cell_aliquot_volume_ul",
    "cell_volume_ul",
    "dna_volume_ul",
    "recovery_aliquot_volume_ul",
    "recovery_volume_ul",
    "cold_minutes",
    "heat_shock_temperature_c",
    "heat_shock_minutes",
    "recovery_temperature_c",
    "recovery_minutes",
    "medium_volume_ul",
    "culture_volume_ul",
    "colony_volume_ul",
];
