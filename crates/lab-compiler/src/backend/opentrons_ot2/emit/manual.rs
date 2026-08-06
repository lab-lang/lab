use std::fmt::Write;

use crate::backend::opentrons_ot2::plan::{Ot2ExecutionPlan, Ot2Well};

/// Wells are addressed as plate and well, because a stage may hold several
/// identical plates. The first plate is unnumbered so the common
/// single-plate layout reads naturally.
fn well_list(wells: &[Ot2Well]) -> String {
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

pub(in crate::backend::opentrons_ot2) fn render_manual_protocol(
    manifest: &Ot2ExecutionPlan,
) -> String {
    let mut output = String::new();
    writeln!(output, "# Lab automated plasmid build — manual protocol\n").unwrap();
    writeln!(output, "> Concept protocol generated for `{}`. Review and qualify it for the actual laboratory before execution.\n", manifest.target).unwrap();
    writeln!(output, "## Build summary\n").unwrap();
    writeln!(
        output,
        "- Plasmids assembled: {}",
        manifest.assemblies.len()
    )
    .unwrap();
    writeln!(output, "- Strains built: {}", manifest.strains.len()).unwrap();
    writeln!(output, "- Workflow: Golden Gate assembly → heat-shock transformation → serial dilution and selective plating").unwrap();
    writeln!(output, "- Opentrons API level: {}\n", manifest.api_level).unwrap();

    writeln!(output, "## Stage 1 — Golden Gate assembly\n").unwrap();
    if manifest.assemblies.is_empty() {
        writeln!(
            output,
            "This batch assembles no plasmid. Retrieve every plasmid it transforms from inventory.\n"
        )
        .unwrap();
    } else {
        writeln!(
            output,
            "Keep DNA and enzymes cold. For every reaction, add reagents in the order shown.\n"
        )
        .unwrap();
    }
    for assembly in &manifest.assemblies {
        writeln!(output, "### {}\n", assembly.artifact).unwrap();
        writeln!(
            output,
            "- Reaction wells: {}",
            assembly.assembly_wells.join(", ")
        )
        .unwrap();
        writeln!(
            output,
            "- Final sequence length: {} bp\n",
            assembly.sequence.len()
        )
        .unwrap();
        writeln!(output, "| Reagent | Volume per reaction |").unwrap();
        writeln!(output, "| --- | ---: |").unwrap();
        writeln!(
            output,
            "| Nuclease-free water | {} µL |",
            assembly.water_volume_ul
        )
        .unwrap();
        writeln!(output, "| T4 DNA ligase buffer | 2 µL |").unwrap();
        writeln!(output, "| T4 DNA ligase | 4 µL |").unwrap();
        writeln!(output, "| {} | 2 µL |", assembly.restriction_enzyme).unwrap();
        writeln!(output, "| {} backbone | 2 µL |", assembly.backbone).unwrap();
        for component in &assembly.components {
            writeln!(output, "| {component} | 2 µL |").unwrap();
        }
        writeln!(output, "| **Total** | **20 µL** |\n").unwrap();
    }
    if !manifest.assemblies.is_empty() {
        writeln!(output, "Run 75 cycles of 37 °C for 2 min and 16 °C for 5 min; then 50 °C for 5 min, 80 °C for 10 min, and hold at 4 °C.\n").unwrap();
    }

    writeln!(output, "## Stage 2 — Heat-shock transformation\n").unwrap();
    writeln!(output, "Load the DNA plate as shown, then for each reaction combine 20 µL competent cells with 2 µL of each plasmid. Incubate at 4 °C for 30 min, heat shock at 42 °C for 1 min, return to 4 °C for 2 min, add 60 µL recovery medium, then recover at 37 °C for 60 min.\n").unwrap();
    if !manifest.dna_source_wells.is_empty() {
        writeln!(output, "| Plasmid | DNA plate well |").unwrap();
        writeln!(output, "| --- | --- |").unwrap();
        for (plasmid, well) in &manifest.dna_source_wells {
            writeln!(
                output,
                "| {plasmid} | {} |",
                well_list(std::slice::from_ref(well))
            )
            .unwrap();
        }
        writeln!(output).unwrap();
    }
    writeln!(
        output,
        "| Strain | Host | Plasmids | DNA wells | Culture destination |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | --- | --- | --- |").unwrap();
    for strain in &manifest.strains {
        for reaction in &strain.transformations {
            writeln!(
                output,
                "| {} | {} | {} | {} | {} |",
                strain.artifact,
                strain.host,
                strain.plasmids.join(", "),
                well_list(&reaction.source_wells),
                reaction.culture_well
            )
            .unwrap();
        }
    }
    writeln!(output).unwrap();

    writeln!(output, "## Stage 3 — Serial dilution and plating\n").unwrap();
    writeln!(output, "Pre-load every dilution well with 18 µL recovery medium. Transfer 2 µL culture into dilution 1 and mix. For dilution 2, transfer 2 µL from dilution 1 into 18 µL fresh medium and mix. Plate 4 µL per replicate onto agar containing the listed selection.\n").unwrap();
    writeln!(
        output,
        "| Strain | Selection | Culture | Dilution wells | Agar wells by dilution |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | --- | --- | --- |").unwrap();
    for strain in &manifest.strains {
        for layout in &strain.plating {
            let agar = layout
                .agar_wells
                .iter()
                .map(|wells| well_list(wells))
                .collect::<Vec<_>>()
                .join("; ");
            writeln!(
                output,
                "| {} | {} | {} | {} | {} |",
                strain.artifact,
                strain.selection,
                layout.culture_well,
                well_list(&layout.dilution_wells),
                agar
            )
            .unwrap();
        }
    }
    writeln!(output, "\n## Execution boundary\n").unwrap();
    writeln!(output, "This concept spike allocates one 96-well reaction plate, one DNA plate, one dilution plate, one agar plate, and 24-well source racks. It does not resolve inventory lots, verify DNA concentrations, design overhangs, domesticate internal restriction sites, or qualify the protocol for a specific lab.").unwrap();
    output
}
