//! Construction of the validated, resource-allocated OT-2 execution plan.

use std::collections::BTreeSet;

use crate::ProtocolLairProgram;

use super::constraints::{
    require_plate_capacity, require_tip_capacity, validate_target_constraints,
    validate_uniform_batch_settings,
};
use super::resources::{
    assembly_source_keys, assign_source_wells, plate_wells, transformation_source_keys,
};
use super::trace::analyze_protocol;
use super::{
    API_LEVEL, Ot2ConstructPlan, Ot2ExecutionPlan, Ot2PlanningError, Ot2PlatingPlan,
    Ot2TransformationPlan, REACTION_VOLUME_UL, SUPPORTED_STEPS, TARGET,
};

/// Validate and allocate an OT-2 build without rendering any output files.
///
/// The returned execution plan is the single input shared by the Python,
/// Markdown, and JSON emitters.
pub fn plan_build(protocol: &ProtocolLairProgram) -> Result<Ot2ExecutionPlan, Ot2PlanningError> {
    plan_selected_build(protocol, None)
}

pub(in crate::backend::opentrons_ot2) fn plan_selected_build(
    protocol: &ProtocolLairProgram,
    selected_artifacts: Option<&BTreeSet<String>>,
) -> Result<Ot2ExecutionPlan, Ot2PlanningError> {
    let context = protocol.context();
    let traces = analyze_protocol(protocol, selected_artifacts)?;
    for trace in &traces {
        validate_target_constraints(trace, context)?;
    }
    validate_uniform_batch_settings(&traces, context)?;

    let assembly_well_count = traces
        .iter()
        .map(|trace| usize::from(trace.assembly_replicates(context)))
        .sum::<usize>();
    let transformation_count = traces
        .iter()
        .map(|trace| {
            usize::from(trace.assembly_replicates(context))
                * usize::from(trace.transformation_replicates(context))
        })
        .sum::<usize>();
    require_plate_capacity(
        "assembly and transformation",
        assembly_well_count + transformation_count,
    )?;
    let assembly_tip_count = traces
        .iter()
        .map(|trace| {
            usize::from(trace.assembly_replicates(context)) * (trace.components(context).len() + 6)
        })
        .sum();
    require_tip_capacity("assembly", "p20", assembly_tip_count)?;
    require_tip_capacity("transformation", "p20", transformation_count * 2)?;
    require_tip_capacity("transformation", "p300", transformation_count)?;

    let dilution_count = traces
        .iter()
        .map(|trace| {
            usize::from(trace.assembly_replicates(context))
                * usize::from(trace.transformation_replicates(context))
                * usize::from(trace.serial_dilutions(context))
        })
        .sum::<usize>();
    require_plate_capacity("serial dilution", dilution_count)?;
    let agar_count = traces
        .iter()
        .map(|trace| {
            usize::from(trace.assembly_replicates(context))
                * usize::from(trace.transformation_replicates(context))
                * usize::from(trace.serial_dilutions(context))
                * usize::from(trace.plating_replicates(context))
        })
        .sum::<usize>();
    require_plate_capacity("plating", agar_count)?;
    let plating_tip_count = traces
        .iter()
        .map(|trace| {
            usize::from(trace.assembly_replicates(context))
                * usize::from(trace.transformation_replicates(context))
                * usize::from(trace.serial_dilutions(context))
                * (1 + usize::from(trace.plating_replicates(context)))
        })
        .sum();
    require_tip_capacity("plating", "p20", plating_tip_count)?;

    let assembly_sources = assembly_source_keys(&traces, context);
    let transformation_sources = transformation_source_keys(&traces, context);
    let assembly_source_wells = assign_source_wells("assembly", assembly_sources)?;
    let transformation_source_wells =
        assign_source_wells("transformation", transformation_sources)?;

    let wells = plate_wells();
    let mut assembly_cursor = 0;
    let mut transformation_cursor = assembly_well_count;
    let mut dilution_cursor = 0;
    let mut agar_cursor = 0;
    let mut constructs = Vec::new();
    for trace in traces {
        let artifact = trace.artifact(context);
        let sequence = trace.sequence(context);
        let backbone = trace.backbone(context);
        let components = trace.components(context);
        let restriction_enzyme = trace.restriction_enzyme(context);
        let host = trace.host(context);
        let selection = trace.selection(context);
        let assembly_replicates = trace.assembly_replicates(context);
        let transformation_replicates = trace.transformation_replicates(context);
        let plating_replicates = trace.plating_replicates(context);
        let serial_dilutions = trace.serial_dilutions(context);
        let assembly_wells =
            wells[assembly_cursor..assembly_cursor + usize::from(assembly_replicates)].to_vec();
        assembly_cursor += assembly_wells.len();

        let mut transformations = Vec::new();
        let mut plating = Vec::new();
        for assembly_well in &assembly_wells {
            for _ in 0..transformation_replicates {
                let culture_well = wells[transformation_cursor].clone();
                transformation_cursor += 1;
                transformations.push(Ot2TransformationPlan {
                    assembly_well: assembly_well.clone(),
                    culture_well: culture_well.clone(),
                });

                let dilution_end = dilution_cursor + usize::from(serial_dilutions);
                let dilution_wells = wells[dilution_cursor..dilution_end].to_vec();
                dilution_cursor = dilution_end;
                let mut agar_wells = Vec::new();
                for _ in 0..serial_dilutions {
                    let agar_end = agar_cursor + usize::from(plating_replicates);
                    agar_wells.push(wells[agar_cursor..agar_end].to_vec());
                    agar_cursor = agar_end;
                }
                plating.push(Ot2PlatingPlan {
                    culture_well,
                    dilution_wells,
                    agar_wells,
                });
            }
        }

        let dna_components = 1 + components.len();
        let required_ul = dna_components as u16 * 2 + 8;
        constructs.push(Ot2ConstructPlan {
            artifact,
            sequence,
            backbone,
            components,
            steps: SUPPORTED_STEPS.into_iter().map(str::to_owned).collect(),
            restriction_enzyme,
            host,
            selection,
            assembly_replicates,
            transformation_replicates,
            plating_replicates,
            serial_dilutions,
            water_volume_ul: REACTION_VOLUME_UL - required_ul,
            assembly_wells,
            transformations,
            plating,
        });
    }

    Ok(Ot2ExecutionPlan {
        schema_version: "lab.automation.v0".into(),
        target: TARGET.into(),
        api_level: API_LEVEL.into(),
        assembly_source_wells,
        transformation_source_wells,
        constructs,
    })
}

#[cfg(test)]
mod tests {
    use lab_language::compile_module;

    use crate::PortableLairProgram;

    use super::*;
    use crate::backend::opentrons_ot2::compile_build;

    const SOURCE: &str = r#"
use std.bio.build
use std.bio.inventory
use std.lab.plasmid_actions

J23101 = part("J23101")
B0034 = part("B0034")
GFP = part("GFP")
B0015 = part("B0015")
pSB1C3 = backbone("pSB1C3")
BsaI = restriction_enzyme("BsaI")
DH5alpha = strain("DH5alpha")
chloramphenicol = antibiotic("chloramphenicol")

plasmid p_gfp:
  sequence: dna("ACGT")
  backbone: pSB1C3
  components: [J23101, B0034, GFP, B0015]
  restriction_enzyme: BsaI
  host: DH5alpha
  selection: chloramphenicol
  assembly_replicates: 1
  transformation_replicates: 2
  plating_replicates: 2
  serial_dilutions: 2
  require topology == circular
  accept sequence == design.sequence

workflow realize_p_gfp() -> (
  product: Material<Plasmid>,
  plate: Material<Plate>,
):
  dependencies = []
  product, construct <- realize p_gfp from dependencies
  cells <- provision DH5alpha
  culture <- transform construct into cells
  culture <- recover culture for 1 h
  culture <- dilute culture
  plate <- plate culture on chloramphenicol
  return product, plate
"#;

    #[test]
    fn compiles_all_three_protocol_stages() {
        let checked = compile_module(SOURCE).unwrap();
        let protocol = PortableLairProgram::lower(&checked)
            .unwrap()
            .select_protocol()
            .unwrap();
        let plan = plan_build(&protocol).unwrap();
        let bundle = compile_build(&protocol).unwrap();

        assert_eq!(bundle.manifest(), &plan);
        assert_eq!(bundle.manifest().constructs[0].assembly_wells, ["A1"]);
        assert_eq!(
            bundle.manifest().constructs[0]
                .transformations
                .iter()
                .map(|reaction| reaction.culture_well.as_str())
                .collect::<Vec<_>>(),
            ["B1", "C1"]
        );
        assert!(
            bundle
                .manual_protocol()
                .contains("Stage 3 — Serial dilution and plating")
        );
        assert!(bundle.assembly_protocol().contains("repetitions=75"));
        assert!(
            bundle
                .transformation_protocol()
                .contains("hold_time_minutes=30")
        );
        assert!(bundle.plating_protocol().contains("p300.distribute("));
        assert_eq!(bundle.artifacts().len(), 5);
        assert!(bundle.artifacts().get("automation_manifest.json").is_some());
        assert!(bundle.artifacts().get("manual_protocol.md").is_some());
    }

    #[test]
    fn rejects_an_invalid_material_chain_before_target_planning() {
        let source = SOURCE.replace(
            "  culture <- recover culture for 1 h\n  culture <- dilute culture\n",
            "",
        );
        let checked = compile_module(&source).unwrap();
        let error = PortableLairProgram::lower(&checked)
            .err()
            .expect("an invalid Workflow material chain must fail LAIR verification");
        assert!(
            error.to_string().contains("Workflow material type"),
            "{error}"
        );
    }

    #[test]
    fn target_metadata_requires_resolved_symbols_of_the_right_kind() {
        for source in [
            SOURCE.replace("  backbone: pSB1C3\n", "  backbone: \"pSB1C3\"\n"),
            SOURCE.replace("  backbone: pSB1C3\n", "  backbone: J23101\n"),
        ] {
            let checked = compile_module(&source).unwrap();
            let error = PortableLairProgram::lower(&checked)
                .err()
                .expect("invalid backbone must fail source-to-LAIR lowering");
            assert!(error.to_string().contains("backbone"), "{error}");
        }
    }
}
