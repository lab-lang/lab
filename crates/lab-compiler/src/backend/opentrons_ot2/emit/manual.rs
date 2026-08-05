use std::fmt::Write;

use super::super::Ot2ExecutionPlan;

pub(in crate::backend::opentrons_ot2) fn render_manual_protocol(
    manifest: &Ot2ExecutionPlan,
) -> String {
    let mut output = String::new();
    writeln!(output, "# Lab automated plasmid build — manual protocol\n").unwrap();
    writeln!(output, "> Concept protocol generated for `{}`. Review and qualify it for the actual laboratory before execution.\n", manifest.target).unwrap();
    writeln!(output, "## Build summary\n").unwrap();
    writeln!(output, "- Constructs: {}", manifest.constructs.len()).unwrap();
    writeln!(output, "- Workflow: Golden Gate assembly → heat-shock transformation → serial dilution and selective plating").unwrap();
    writeln!(output, "- Opentrons API level: {}\n", manifest.api_level).unwrap();

    writeln!(output, "## Stage 1 — Golden Gate assembly\n").unwrap();
    writeln!(
        output,
        "Keep DNA and enzymes cold. For every reaction, add reagents in the order shown.\n"
    )
    .unwrap();
    for construct in &manifest.constructs {
        writeln!(output, "### {}\n", construct.artifact).unwrap();
        writeln!(
            output,
            "- Reaction wells: {}",
            construct.assembly_wells.join(", ")
        )
        .unwrap();
        writeln!(
            output,
            "- Final sequence length: {} bp\n",
            construct.sequence.len()
        )
        .unwrap();
        writeln!(output, "| Reagent | Volume per reaction |").unwrap();
        writeln!(output, "| --- | ---: |").unwrap();
        writeln!(
            output,
            "| Nuclease-free water | {} µL |",
            construct.water_volume_ul
        )
        .unwrap();
        writeln!(output, "| T4 DNA ligase buffer | 2 µL |").unwrap();
        writeln!(output, "| T4 DNA ligase | 4 µL |").unwrap();
        writeln!(output, "| {} | 2 µL |", construct.restriction_enzyme).unwrap();
        writeln!(output, "| {} backbone | 2 µL |", construct.backbone).unwrap();
        for component in &construct.components {
            writeln!(output, "| {component} | 2 µL |").unwrap();
        }
        writeln!(output, "| **Total** | **20 µL** |\n").unwrap();
    }
    writeln!(output, "Run 75 cycles of 37 °C for 2 min and 16 °C for 5 min; then 50 °C for 5 min, 80 °C for 10 min, and hold at 4 °C.\n").unwrap();

    writeln!(output, "## Stage 2 — Heat-shock transformation\n").unwrap();
    writeln!(output, "For each mapping below, combine 20 µL competent cells with 2 µL assembly product. Incubate at 4 °C for 30 min, heat shock at 42 °C for 1 min, return to 4 °C for 2 min, add 60 µL recovery medium, then recover at 37 °C for 60 min.\n").unwrap();
    writeln!(
        output,
        "| Construct | Host | Assembly source | Culture destination |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | --- | --- |").unwrap();
    for construct in &manifest.constructs {
        for reaction in &construct.transformations {
            writeln!(
                output,
                "| {} | {} | {} | {} |",
                construct.artifact, construct.host, reaction.assembly_well, reaction.culture_well
            )
            .unwrap();
        }
    }
    writeln!(output).unwrap();

    writeln!(output, "## Stage 3 — Serial dilution and plating\n").unwrap();
    writeln!(output, "Pre-load every dilution well with 18 µL recovery medium. Transfer 2 µL culture into dilution 1 and mix. For dilution 2, transfer 2 µL from dilution 1 into 18 µL fresh medium and mix. Plate 4 µL per replicate onto agar containing the listed selection.\n").unwrap();
    writeln!(
        output,
        "| Construct | Selection | Culture | Dilution wells | Agar wells by dilution |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | --- | --- | --- |").unwrap();
    for construct in &manifest.constructs {
        for layout in &construct.plating {
            let agar = layout
                .agar_wells
                .iter()
                .map(|wells| wells.join(", "))
                .collect::<Vec<_>>()
                .join("; ");
            writeln!(
                output,
                "| {} | {} | {} | {} | {} |",
                construct.artifact,
                construct.selection,
                layout.culture_well,
                layout.dilution_wells.join(", "),
                agar
            )
            .unwrap();
        }
    }
    writeln!(output, "\n## Execution boundary\n").unwrap();
    writeln!(output, "This concept spike allocates one 96-well reaction plate, one dilution plate, one agar plate, and 24-well source racks. It does not resolve inventory lots, verify DNA concentrations, design overhangs, domesticate internal restriction sites, or qualify the protocol for a specific lab.").unwrap();
    output
}
