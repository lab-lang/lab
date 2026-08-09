use std::fmt::Write;

use crate::backend::opentrons::ot2::plan::{Ot2ExecutionPlan, Ot2Well};

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

pub(in crate::backend::opentrons::ot2) fn render_manual_protocol(
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
        writeln!(
            output,
            "| T4 DNA ligase buffer | {} µL |",
            assembly.chemistry.buffer_volume_ul
        )
        .unwrap();
        writeln!(
            output,
            "| T4 DNA ligase | {} µL |",
            assembly.chemistry.ligase_volume_ul
        )
        .unwrap();
        writeln!(
            output,
            "| {} | {} µL |",
            assembly.restriction_enzyme, assembly.chemistry.enzyme_volume_ul
        )
        .unwrap();
        writeln!(
            output,
            "| {} backbone | {} µL |",
            assembly.backbone, assembly.chemistry.part_volume_ul
        )
        .unwrap();
        for component in &assembly.components {
            writeln!(
                output,
                "| {component} | {} µL |",
                assembly.chemistry.part_volume_ul
            )
            .unwrap();
        }
        writeln!(
            output,
            "| **Total** | **{} µL** |\n",
            assembly.chemistry.reaction_volume_ul
        )
        .unwrap();
    }
    // Every assembly in a batch shares one thermal profile, driven by the
    // first construct — see assembly.py.
    if let Some(assembly) = manifest.assemblies.first() {
        let chemistry = &assembly.chemistry;
        writeln!(
            output,
            "Run {} cycles of {} °C for {} min and {} °C for {} min; then 50 °C for 5 min, 80 °C for 10 min, and hold at 4 °C.\n",
            chemistry.cycles,
            chemistry.digest_temperature_c,
            chemistry.digest_minutes,
            chemistry.ligate_temperature_c,
            chemistry.ligate_minutes,
        )
        .unwrap();
    }

    writeln!(output, "## Stage 2 — Heat-shock transformation\n").unwrap();
    writeln!(output, "Load the DNA plate as shown, then for each reaction combine that strain's cells and plasmid DNA in the volumes listed below.\n").unwrap();
    // Every strain in a batch shares one heat-shock profile, driven by the
    // first strain — see transformation.py.
    if let Some(strain) = manifest.strains.first() {
        let chemistry = &strain.chemistry;
        writeln!(
            output,
            "Incubate at 4 °C for {} min, heat shock at {} °C for {} min, return to 4 °C for 2 min, add recovery medium in the volume listed below, then recover at {} °C for {} min.\n",
            chemistry.cold_minutes,
            chemistry.heat_shock_temperature_c,
            chemistry.heat_shock_minutes,
            chemistry.recovery_temperature_c,
            chemistry.recovery_minutes,
        )
        .unwrap();
    }
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
        "| Strain | Host | Plasmids | DNA wells | Culture destination | Cells (µL) | DNA per plasmid (µL) | Recovery medium (µL) |"
    )
    .unwrap();
    writeln!(
        output,
        "| --- | --- | --- | --- | --- | ---: | ---: | ---: |"
    )
    .unwrap();
    for strain in &manifest.strains {
        for reaction in &strain.transformations {
            writeln!(
                output,
                "| {} | {} | {} | {} | {} | {} | {} | {} |",
                strain.artifact,
                strain.host,
                strain.plasmids.join(", "),
                well_list(&reaction.source_wells),
                reaction.culture_well,
                strain.chemistry.cell_volume_ul,
                strain.chemistry.dna_volume_ul,
                strain.chemistry.recovery_volume_ul,
            )
            .unwrap();
        }
    }
    writeln!(output).unwrap();

    writeln!(output, "## Stage 3 — Serial dilution and plating\n").unwrap();
    // The dilution-well pre-load happens once for the whole batch, driven by
    // the first strain — see plating.py.
    if let Some(strain) = manifest.strains.first() {
        writeln!(
            output,
            "Pre-load every dilution well with {} µL recovery medium. For each serial dilution, transfer culture from the previous well (or the transformation culture, for the first dilution) and mix, then plate onto agar containing the listed selection, using the volumes listed below.\n",
            strain.chemistry.medium_volume_ul
        )
        .unwrap();
    }
    writeln!(
        output,
        "| Strain | Selection | Culture | Dilution wells | Agar wells by dilution | Culture transfer (µL) | Colony transfer (µL) |"
    )
    .unwrap();
    writeln!(output, "| --- | --- | --- | --- | --- | ---: | ---: |").unwrap();
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
                "| {} | {} | {} | {} | {} | {} | {} |",
                strain.artifact,
                strain.selection,
                layout.culture_well,
                well_list(&layout.dilution_wells),
                agar,
                strain.chemistry.culture_volume_ul,
                strain.chemistry.colony_volume_ul,
            )
            .unwrap();
        }
    }
    writeln!(output, "\n## Execution boundary\n").unwrap();
    writeln!(output, "This concept spike allocates one 96-well reaction plate, one DNA plate, one dilution plate, one agar plate, and 24-well source racks. It does not resolve inventory lots, verify DNA concentrations, design overhangs, domesticate internal restriction sites, or qualify the protocol for a specific lab.").unwrap();
    output
}

#[cfg(test)]
mod tests {
    use lab_language::compile_module;

    use crate::PortableLairProgram;
    use crate::backend::opentrons::ot2::emit::manual::*;
    use crate::backend::opentrons::ot2::plan_build;
    use crate::backend::opentrons::ot2::profile::Ot2TargetProfile;

    const SOURCE: &str = r#"
use std.bio.build
use std.bio.designs
use std.bio.golden_gate
use std.lab.plasmid

buy part J23101
buy part B0034
buy part GFP
buy part B0015
buy backbone pSB1C3
buy restriction_enzyme BsaI
buy chassis DH5alpha
buy antibiotic chloramphenicol

plasmid p_gfp:
  sequence = dna("ACGT")
  backbone = pSB1C3
  components = [J23101, B0034, GFP, B0015]
  restriction_enzyme = BsaI
  assembly_replicates = 1
  reaction_volume = 30 uL
  part_volume = 3 uL
  assembly_cycles = 40
  require topology == circular
  accept sequence == design.sequence

strain reporter_host:
  chassis = DH5alpha
  plasmids = [p_gfp]
  selection = chloramphenicol
  transformation_replicates = 1
  plating_replicates = 1
  serial_dilutions = 1
  heat_shock_temperature = 45 C
  colony_volume = 6 uL

workflow assemble_p_gfp() -> Material<Plasmid>:
  dependencies = []
  product <- realize p_gfp from dependencies
  return product

workflow build_reporter_host(
  p_gfp: Material<Plasmid>,
) -> (
  strain: Material<Strain>,
  plate: Material<Plate>,
):
  dependencies = [p_gfp]
  cells <- provision DH5alpha
  strain, culture <- transform reporter_host from dependencies into cells
  culture <- recover culture for 1 h
  culture <- dilute culture
  plate <- plate culture on chloramphenicol
  return strain, plate
"#;

    #[test]
    fn manual_protocol_reflects_overridden_chemistry_instead_of_reference_defaults() {
        let checked = compile_module(SOURCE).unwrap();
        let protocol = PortableLairProgram::lower(&checked)
            .unwrap()
            .select_protocol()
            .unwrap();
        let plan = plan_build(&protocol, &Ot2TargetProfile::default()).unwrap();
        let manual = render_manual_protocol(&plan);

        assert!(
            manual.contains("Run 40 cycles"),
            "overridden assembly_cycles must reach the manual instead of the reference default of 75"
        );
        assert!(!manual.contains("Run 75 cycles"));
        assert!(
            manual.contains("**30 µL**"),
            "overridden reaction_volume must reach the manual instead of the reference default of 20 µL"
        );
        assert!(
            manual.contains("heat shock at 45 °C"),
            "overridden heat_shock_temperature must reach the manual instead of the reference default of 42 °C"
        );
        assert!(!manual.contains("heat shock at 42 °C"));
        assert!(
            manual.contains("| 6 |"),
            "overridden colony_volume must reach the plating table instead of the reference default of 4"
        );
    }
}
