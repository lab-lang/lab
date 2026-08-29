//! The operator manual, in three fragments. Bench sections describe the
//! machine and deck and hold for every run on the profile; run sections
//! carry one execution plan's sources, volumes, and stages; the boundary
//! section states what this concept protocol does not cover. A standalone
//! manual composes all three, and the stitched full-build document renders
//! the bench sections once and splices only run sections per wave.

use crate::backend::document::{Block, Column, Doc, DocMeta, bold, code, text};
use crate::backend::opentrons::flex::plan::{FlexExecutionPlan, FlexWell};
use crate::backend::opentrons::flex::profile::FlexAdapterProfile;

/// Wells are addressed as plate and well, because a stage may hold several
/// identical plates. The first plate is unnumbered so the common
/// single-plate layout reads naturally.
fn well_list(wells: &[FlexWell]) -> String {
    wells
        .iter()
        .map(|well| {
            if well.plate == 0 {
                well.well.clone()
            } else {
                format!("plate {} {}", well.plate + 1, well.well)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn fragment() -> Doc {
    Doc::new(DocMeta::new("", "", "", ""))
}

/// The machine, its modules, and the deck: everything that holds for any
/// run compiled against this bench profile.
pub(in crate::backend) fn bench_blocks(deck: &FlexAdapterProfile) -> Vec<Block> {
    let mut doc = fragment();

    doc.heading(1, [text("How Lab and the robot divide the work")]);
    doc.para([
        text("The manuals and the robot protocols in this package were generated together by "),
        code("lab build"),
        text(" as projections of one execution plan, so the wells, volumes, and deck positions printed here are exactly what the robot will do. Regenerate them from the "),
        code(".lab"),
        text(" sources rather than editing either."),
    ]);
    doc.para_text(
        "The robot performs every liquid transfer and every temperature step: reagent distribution from the chilled rack, the assembly thermal profile, cold incubation and heat shock, recovery holds, serial dilution, and agar spotting. The operator works between runs, loading labware, refilling sources and tips, and starting each stage. After the final run the selective plates leave the deck for incubation. Do not touch the deck while a run is in progress.",
    );

    doc.heading(1, [text("Machine setup")]);

    doc.heading(2, [text("Power and connection")]);
    doc.para_text(
        "Switch the Flex on at the rear power switch and wait for the touchscreen to reach its home screen. Work from the touchscreen or from the Opentrons App on a computer on the same network, whichever the lab prefers; both accept the protocol files in this package. If this bench has not been calibrated recently, run the instrument attachment and calibration flows before loading anything.",
    );
    doc.para([
        text("Each stage of a run is one standalone Python protocol ("),
        code("*_protocol.py"),
        text(") with its entire execution plan embedded. Import the file into the app; it needs nothing else, and the app previews every step before you press start."),
    ]);

    doc.heading(2, [text("Instruments and modules")]);
    doc.para_text(
        "The bench profile expects the following pipettes and modules. Check the mounts before the first run; a pipette on the wrong mount fails in the app's setup checks rather than silently.",
    );
    doc.table(
        [
            Column::left("Pipette"),
            Column::left("Model"),
            Column::left("Mount"),
        ],
        [
            vec![
                vec![text("Small")],
                vec![code(deck.instruments.small.model.as_str())],
                vec![text(deck.instruments.small.mount.as_str())],
            ],
            vec![
                vec![text("Large")],
                vec![code(deck.instruments.large.model.as_str())],
                vec![text(deck.instruments.large.mount.as_str())],
            ],
        ],
    );
    doc.table(
        [
            Column::left("Module"),
            Column::left("Model"),
            Column::left("Placement"),
            Column::left("Carries"),
        ],
        [
            vec![
                vec![text("Temperature module")],
                vec![code(deck.deck.temperature_module.model.as_str())],
                vec![text(format!("slot {}", deck.deck.temperature_module.slot))],
                vec![
                    code(deck.deck.temperature_module.labware.as_str()),
                    text(" (the chilled source rack)"),
                ],
            ],
            vec![
                vec![text("Thermocycler")],
                vec![code(deck.deck.thermocycler.model.as_str())],
                vec![text("its fixed deck position")],
                vec![
                    code(deck.deck.thermocycler.labware.as_str()),
                    text(" (the reaction plate, in place across all three stages)"),
                ],
            ],
        ],
    );

    doc.heading(2, [text("Deck layout")]);
    doc.para_text(
        "Load each stage's labware before starting its run; earlier stages' tip racks can come off once their stage is complete. The reaction plate stays on the thermocycler throughout.",
    );
    let mut deck_rows: Vec<Vec<Vec<crate::backend::document::Inline>>> = Vec::new();
    let mut deck_row = |stage: &str, resource: &str, slots: String, labware: &str| {
        deck_rows.push(vec![
            vec![text(stage)],
            vec![text(resource)],
            vec![text(slots)],
            vec![code(labware)],
        ]);
    };
    deck_row(
        "assembly",
        "small tips",
        deck.stages.assembly.small_tips.slots.join(", "),
        &deck.stages.assembly.small_tips.labware,
    );
    deck_row(
        "transformation",
        "DNA plate",
        deck.stages.transformation.dna_plate.slots.join(", "),
        &deck.stages.transformation.dna_plate.labware,
    );
    deck_row(
        "transformation",
        "small tips",
        deck.stages.transformation.small_tips.slots.join(", "),
        &deck.stages.transformation.small_tips.labware,
    );
    deck_row(
        "transformation",
        "large tips",
        deck.stages.transformation.large_tips.slots.join(", "),
        &deck.stages.transformation.large_tips.labware,
    );
    deck_row(
        "plating",
        "dilution plate",
        deck.stages.plating.dilution_plate.slots.join(", "),
        &deck.stages.plating.dilution_plate.labware,
    );
    deck_row(
        "plating",
        "agar plate",
        deck.stages.plating.agar_plate.slots.join(", "),
        &deck.stages.plating.agar_plate.labware,
    );
    deck_row(
        "plating",
        "media rack",
        deck.stages.plating.media_rack.slot.clone(),
        &deck.stages.plating.media_rack.labware,
    );
    deck_row(
        "plating",
        "small tips",
        deck.stages.plating.small_tips.slots.join(", "),
        &deck.stages.plating.small_tips.labware,
    );
    deck_row(
        "plating",
        "large tips",
        deck.stages.plating.large_tips.slots.join(", "),
        &deck.stages.plating.large_tips.labware,
    );
    doc.table(
        [
            Column::left("Stage"),
            Column::left("Resource"),
            Column::left("Deck slots"),
            Column::left("Labware"),
        ],
        deck_rows,
    );

    doc.blocks
}

/// One execution plan's manual content: summary, sources, and the three
/// stages in order.
pub(in crate::backend) fn run_blocks(manifest: &FlexExecutionPlan) -> Vec<Block> {
    let mut doc = fragment();

    doc.heading(1, [text("Build summary")]);
    doc.bullets([
        vec![text(format!(
            "Plasmids assembled: {}",
            manifest.assemblies.len()
        ))],
        vec![text(format!("Strains built: {}", manifest.strains.len()))],
        vec![text(
            "Workflow: Golden Gate assembly, heat-shock transformation, then serial dilution and selective plating",
        )],
        vec![text("Protocol format: Opentrons JSON protocol (schema 8)")],
    ]);

    if !manifest.assembly_source_wells.is_empty()
        || !manifest.transformation_source_wells.is_empty()
    {
        doc.heading(1, [text("Source loading")]);
        doc.para_text(
            "Sources sit in the chilled rack on the temperature module. The rack is reloaded between stages: empty it after assembly and load the transformation sources before starting that run. Keep DNA and enzymes cold while loading.",
        );
        doc.table(
            [
                Column::left("Stage"),
                Column::left("Contents"),
                Column::left("Chilled rack well"),
            ],
            manifest
                .assembly_source_wells
                .iter()
                .map(|(contents, well)| ("assembly", contents, well))
                .chain(
                    manifest
                        .transformation_source_wells
                        .iter()
                        .map(|(contents, well)| ("transformation", contents, well)),
                )
                .map(|(stage, contents, well)| {
                    vec![
                        vec![text(stage)],
                        vec![text(contents.as_str())],
                        vec![text(well.as_str())],
                    ]
                }),
        );
    }

    doc.labeled_heading(1, "Stage 1", [text("Golden Gate assembly")]);
    if manifest.assemblies.is_empty() {
        doc.para_text(
            "This batch assembles no plasmid. Retrieve every plasmid it transforms from inventory.",
        );
    } else {
        doc.para([
            text("With the assembly sources chilled and the assembly tips loaded, import "),
            code("assembly_protocol.json"),
            text(" into the app and start the run. The robot builds every reaction listed below itself, adding water, buffer, ligase, enzyme, backbone, then each part, in that order. The tables here are for review and for source preparation, not for hand pipetting."),
        ]);
    }
    for assembly in &manifest.assemblies {
        doc.heading(2, [code(assembly.artifact.as_str())]);
        doc.bullets([
            vec![text(format!(
                "Reaction wells: {}",
                assembly.assembly_wells.join(", ")
            ))],
            vec![text(format!(
                "Final sequence length: {} bp",
                assembly.sequence.len()
            ))],
        ]);
        let mut rows = vec![
            vec![
                vec![text("Nuclease-free water")],
                vec![text(format!("{} µL", assembly.water_volume_ul))],
            ],
            vec![
                vec![text("T4 DNA ligase buffer")],
                vec![text(format!("{} µL", assembly.chemistry.buffer_volume_ul))],
            ],
            vec![
                vec![text("T4 DNA ligase")],
                vec![text(format!("{} µL", assembly.chemistry.ligase_volume_ul))],
            ],
            vec![
                vec![text(assembly.restriction_enzyme.as_str())],
                vec![text(format!("{} µL", assembly.chemistry.enzyme_volume_ul))],
            ],
            vec![
                vec![text(format!("{} backbone", assembly.backbone))],
                vec![text(format!("{} µL", assembly.chemistry.part_volume_ul))],
            ],
        ];
        for component in &assembly.components {
            rows.push(vec![
                vec![text(component.as_str())],
                vec![text(format!("{} µL", assembly.chemistry.part_volume_ul))],
            ]);
        }
        rows.push(vec![
            vec![bold("Total")],
            vec![bold(format!(
                "{} µL",
                assembly.chemistry.reaction_volume_ul
            ))],
        ]);
        doc.table(
            [
                Column::left("Reagent"),
                Column::right("Volume per reaction"),
            ],
            rows,
        );
    }
    // Every assembly in a batch shares one thermal profile, driven by the
    // first construct; see assembly_protocol.json.
    if let Some(assembly) = manifest.assemblies.first() {
        let chemistry = &assembly.chemistry;
        doc.para_text(format!(
            "After the transfers, the robot closes the thermocycler and runs {} cycles of {} °C for {} min and {} °C for {} min; then 50 °C for 5 min, 80 °C for 10 min, and a hold at 4 °C. No operator action is needed while the profile runs. When the run reports complete, leave the reaction plate on the open thermocycler; transformation continues on the same plate.",
            chemistry.cycles,
            chemistry.digest_temperature_c,
            chemistry.digest_minutes,
            chemistry.ligate_temperature_c,
            chemistry.ligate_minutes,
        ));
    }

    doc.labeled_heading(1, "Stage 2", [text("Heat-shock transformation")]);
    if manifest.strains.is_empty() {
        doc.para_text(
            "This batch transforms no strain, so it emits no transformation protocol. Preserve the reaction plate for the wave that consumes these plasmids.",
        );
    } else {
        doc.para([
        text("Reload the chilled rack with the transformation sources, load the DNA plate as shown below, and swap in this stage's tip racks. Then import "),
        code("transformation_protocol.json"),
        text(" and start the run: for each reaction the robot combines that strain's cells and plasmid DNA in the volumes listed below. A plasmid assembled in stage 1 is drawn from the reaction plate automatically; only plasmids retrieved from inventory need loading onto the DNA plate."),
    ]);
        // Every strain in a batch shares one heat-shock profile, driven by the
        // first strain; see transformation_protocol.json.
        if let Some(strain) = manifest.strains.first() {
            let chemistry = &strain.chemistry;
            doc.para_text(format!(
            "The thermocycler then carries the whole temperature sequence unattended: incubate at 4 °C for {} min, heat shock at {} °C for {} min, return to 4 °C for 2 min, add recovery medium in the volume listed below, then recover at {} °C for {} min. When the run reports complete, leave the reaction plate in place for plating.",
            chemistry.cold_minutes,
            chemistry.heat_shock_temperature_c,
            chemistry.heat_shock_minutes,
            chemistry.recovery_temperature_c,
            chemistry.recovery_minutes,
        ));
        }
        if !manifest.dna_source_wells.is_empty() {
            doc.table(
                [Column::left("Plasmid"), Column::left("DNA plate well")],
                manifest.dna_source_wells.iter().map(|(plasmid, well)| {
                    vec![
                        vec![text(plasmid.as_str())],
                        vec![text(well_list(std::slice::from_ref(well)))],
                    ]
                }),
            );
        }
        doc.table(
            [
                Column::left("Strain"),
                Column::left("Host"),
                Column::left("Plasmids"),
                Column::left("DNA wells"),
                Column::left("Culture well"),
                Column::right("Cells (µL)"),
                Column::right("DNA / plasmid (µL)"),
                Column::right("Recovery (µL)"),
            ],
            manifest.strains.iter().flat_map(|strain| {
                strain.transformations.iter().map(|reaction| {
                    vec![
                        vec![text(strain.artifact.as_str())],
                        vec![text(strain.host.as_str())],
                        vec![text(strain.plasmids.join(", "))],
                        vec![text(well_list(&reaction.source_wells))],
                        vec![text(reaction.culture_well.as_str())],
                        vec![text(strain.chemistry.cell_volume_ul.to_string())],
                        vec![text(strain.chemistry.dna_volume_ul.to_string())],
                        vec![text(strain.chemistry.recovery_volume_ul.to_string())],
                    ]
                })
            }),
        );
    }

    doc.labeled_heading(1, "Stage 3", [text("Serial dilution and plating")]);
    if manifest.strains.is_empty() {
        doc.para_text(
            "With no strain in this batch there is nothing to dilute or plate, so this stage emits no protocol.",
        );
    } else {
        // The dilution-well pre-load happens once for the whole batch, driven by
        // the first strain; see plating_protocol.json.
        if let Some(strain) = manifest.strains.first() {
            doc.para([
            text("Load the dilution plate, the selective agar plates, the media rack, and this stage's tips, then import "),
            code("plating_protocol.json"),
            text(format!(
                " and start the run. The robot pre-loads every dilution well with {} µL recovery medium, carries each serial dilution by transferring culture from the previous well (or the transformation culture, for the first dilution) and mixing, then spots the dilutions onto agar containing the listed selection, using the volumes listed below.",
                strain.chemistry.medium_volume_ul
            )),
        ]);
            doc.para_text(
            "When the run completes, remove the agar plates and incubate them under host-appropriate conditions. Colony picking and everything after it happen off the robot.",
        );
        }
        doc.table(
            [
                Column::left("Strain"),
                Column::left("Selection"),
                Column::left("Culture"),
                Column::left("Dilution wells"),
                Column::left("Agar wells by dilution"),
                Column::right("Culture transfer (µL)"),
                Column::right("Colony transfer (µL)"),
            ],
            manifest.strains.iter().flat_map(|strain| {
                strain.plating.iter().map(|layout| {
                    let agar = layout
                        .agar_wells
                        .iter()
                        .map(|wells| well_list(wells))
                        .collect::<Vec<_>>()
                        .join("; ");
                    vec![
                        vec![text(strain.artifact.as_str())],
                        vec![text(strain.selection.as_str())],
                        vec![text(layout.culture_well.as_str())],
                        vec![text(well_list(&layout.dilution_wells))],
                        vec![text(agar)],
                        vec![text(strain.chemistry.culture_volume_ul.to_string())],
                        vec![text(strain.chemistry.colony_volume_ul.to_string())],
                    ]
                })
            }),
        );
    }

    doc.blocks
}

/// The standing caveat on what this concept protocol does not resolve.
pub(in crate::backend) fn boundary_blocks() -> Vec<Block> {
    let mut doc = fragment();
    doc.heading(1, [text("Execution boundary")]);
    doc.para_text(
        "This concept spike allocates one 96-well reaction plate, one DNA plate, one dilution plate, one agar plate, and 24-well source racks. Dependency planning may freeze exact inventory lots, but this device plan does not reason over their quantities, verify DNA concentrations, design overhangs, domesticate internal restriction sites, or qualify the protocol for a specific lab.",
    );
    doc.blocks
}

pub(in crate::backend) fn render_manual_protocol(manifest: &FlexExecutionPlan) -> Doc {
    let mut doc = Doc::new(DocMeta::new(
        "Automated plasmid build",
        "Operator manual for one robot session",
        &manifest.adapter,
        "Opentrons Flex",
    ));
    doc.notice([
        text("Concept protocol generated for "),
        code(&manifest.adapter),
        text(". Review and qualify it for the actual laboratory before execution."),
    ]);
    doc.blocks.extend(bench_blocks(&manifest.deck));
    doc.blocks.extend(run_blocks(manifest));
    doc.blocks.extend(boundary_blocks());
    doc
}
