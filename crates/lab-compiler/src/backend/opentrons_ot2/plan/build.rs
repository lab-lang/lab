//! Construction of the validated, resource-allocated OT-2 execution plan.

use std::collections::{BTreeMap, BTreeSet};

use pliron::context::Context;

use crate::ProtocolLairProgram;
use crate::backend::opentrons_ot2::profile::{Ot2TargetProfile, Plates, Stages};

use super::constraints::{
    require_tip_capacity, validate_assembly_constraints, validate_strain_constraints,
    validate_uniform_batch_settings,
};
use super::resources::{
    PlateAllocator, assembly_source_keys, assign_source_wells, require_known_geometry,
    transformation_source_keys,
};
use super::trace::{AssemblyTrace, ProtocolTraces, StrainTrace, analyze_protocol};
use super::{
    Ot2AssemblyChemistry, Ot2AssemblyPlan, Ot2ExecutionPlan, Ot2PlanningError, Ot2PlatingPlan,
    Ot2StrainChemistry, Ot2StrainPlan, Ot2TransformationPlan, Ot2Well,
};
use crate::backend::opentrons_ot2::BACKEND;

/// Validate and allocate an OT-2 build against one bench without rendering any
/// output files.
///
/// The returned execution plan is the single input shared by the Python,
/// Markdown, and JSON emitters.
pub fn plan_build(
    protocol: &ProtocolLairProgram,
    profile: &Ot2TargetProfile,
) -> Result<Ot2ExecutionPlan, Ot2PlanningError> {
    plan_selected_build(protocol, profile, None)
}

pub(in crate::backend::opentrons_ot2) fn plan_selected_build(
    protocol: &ProtocolLairProgram,
    profile: &Ot2TargetProfile,
    selected_artifacts: Option<&BTreeSet<String>>,
) -> Result<Ot2ExecutionPlan, Ot2PlanningError> {
    let context = protocol.context();
    let traces = analyze_protocol(protocol, selected_artifacts)?;
    validate_traces(&traces, context)?;

    let deck = &profile.deck;
    let stages = &profile.stages;
    require_deck_geometry(profile)?;
    require_stage_tip_capacities(&traces, context, stages)?;

    // The thermocycler holds one plate, so assembly and transformation each
    // address it as a single plate rather than a spillable stage resource.
    let reaction_plate = super::resources::plate_wells(deck.thermocycler.capacity);

    let assemblies = build_assembly_plans(&traces.assemblies, context, &reaction_plate)?;

    // Every distinct plasmid any strain carries occupies one DNA-plate well,
    // whether this batch assembled it or the operator retrieved it.
    let dna_source_wells =
        allocate_dna_wells(&traces.strains, context, &stages.transformation.dna_plate)?;

    // The thermocycler plate is cleared and reused for transformation, so
    // this stage addresses it with its own cursor, independent of assembly's.
    let strains = build_strain_plans(
        &traces.strains,
        context,
        &reaction_plate,
        &dna_source_wells,
        stages,
    )?;

    let rack_capacity = deck.temperature_module.capacity;
    let (assembly_source_wells, transformation_source_wells) =
        assign_all_source_wells(&traces, context, rack_capacity)?;

    Ok(Ot2ExecutionPlan {
        schema_version: "lab.automation.v0".into(),
        target: BACKEND.into(),
        api_level: profile.target.api_level.clone(),
        deck: profile.clone(),
        assembly_source_wells,
        transformation_source_wells,
        dna_source_wells,
        assemblies,
        strains,
    })
}

/// Checks each artifact's own assembly/transformation chemistry, then the
/// settings that must agree across every strain in the batch.
fn validate_traces(traces: &ProtocolTraces, context: &Context) -> Result<(), Ot2PlanningError> {
    for trace in &traces.assemblies {
        validate_assembly_constraints(trace, context)?;
    }
    for trace in &traces.strains {
        validate_strain_constraints(trace, context)?;
    }
    validate_uniform_batch_settings(&traces.strains, context)
}

/// Rejects a target profile that declares labware in a well count this
/// backend has no row/column layout for.
fn require_deck_geometry(profile: &Ot2TargetProfile) -> Result<(), Ot2PlanningError> {
    let deck = &profile.deck;
    let stages = &profile.stages;
    require_known_geometry("the source rack", deck.temperature_module.capacity)?;
    require_known_geometry("the reaction plate", deck.thermocycler.capacity)?;
    require_known_geometry("the DNA plate", stages.transformation.dna_plate.capacity)?;
    require_known_geometry("the dilution plate", stages.plating.dilution_plate.capacity)?;
    require_known_geometry("the agar plate", stages.plating.agar_plate.capacity)
}

/// Checks the small/large tip racks declared for each stage against the tip
/// count that stage's reactions will consume.
fn require_stage_tip_capacities(
    traces: &ProtocolTraces,
    context: &Context,
    stages: &Stages,
) -> Result<(), Ot2PlanningError> {
    let assembly_tip_count = traces
        .assemblies
        .iter()
        .map(|trace| {
            usize::from(trace.assembly_replicates(context)) * (trace.components(context).len() + 6)
        })
        .sum();
    require_tip_capacity(
        "assembly",
        "small",
        assembly_tip_count,
        stages.assembly.small_tips.total_capacity(),
    )?;

    let transformation_count = traces
        .strains
        .iter()
        .map(|trace| usize::from(trace.transformation_replicates(context)))
        .sum::<usize>();
    let transformation_tip_count = traces
        .strains
        .iter()
        .map(|trace| {
            usize::from(trace.transformation_replicates(context))
                * (1 + trace.plasmids(context).len())
        })
        .sum();
    require_tip_capacity(
        "transformation",
        "small",
        transformation_tip_count,
        stages.transformation.small_tips.total_capacity(),
    )?;
    require_tip_capacity(
        "transformation",
        "large",
        transformation_count,
        stages.transformation.large_tips.total_capacity(),
    )?;

    let plating_tip_count = traces
        .strains
        .iter()
        .map(|trace| {
            usize::from(trace.transformation_replicates(context))
                * usize::from(trace.serial_dilutions(context))
                * (1 + usize::from(trace.plating_replicates(context)))
        })
        .sum();
    require_tip_capacity(
        "plating",
        "small",
        plating_tip_count,
        stages.plating.small_tips.total_capacity(),
    )
}

/// Allocates each assembly its reaction wells and computes the chemistry
/// (including the water top-up volume) for every plasmid the batch builds.
fn build_assembly_plans(
    traces: &[AssemblyTrace],
    context: &Context,
    reaction_plate: &[String],
) -> Result<Vec<Ot2AssemblyPlan>, Ot2PlanningError> {
    let mut assembly_cursor = 0;
    let mut assemblies = Vec::new();
    for trace in traces {
        let assembly_replicates = trace.assembly_replicates(context);
        let end = assembly_cursor + usize::from(assembly_replicates);
        if end > reaction_plate.len() {
            return Err(super::constraints::plate_capacity_error(
                "assembly",
                "reaction_plate",
                end,
                reaction_plate.len(),
            ));
        }
        let assembly_wells = reaction_plate[assembly_cursor..end].to_vec();
        assembly_cursor = end;
        let components = trace.components(context);
        let chemistry = Ot2AssemblyChemistry {
            reaction_volume_ul: trace.chemistry(context, "reaction_volume_ul"),
            part_volume_ul: trace.chemistry(context, "part_volume_ul"),
            enzyme_volume_ul: trace.chemistry(context, "enzyme_volume_ul"),
            ligase_volume_ul: trace.chemistry(context, "ligase_volume_ul"),
            buffer_volume_ul: trace.chemistry(context, "buffer_volume_ul"),
            cycles: trace.chemistry(context, "cycles"),
            digest_temperature_c: trace.chemistry(context, "digest_temperature_c"),
            digest_minutes: trace.chemistry(context, "digest_minutes"),
            ligate_temperature_c: trace.chemistry(context, "ligate_temperature_c"),
            ligate_minutes: trace.chemistry(context, "ligate_minutes"),
        };
        // The backbone enters the reaction alongside every component.
        let dna_pieces = (1 + components.len()) as u16;
        let consumed = chemistry.buffer_volume_ul
            + chemistry.ligase_volume_ul
            + chemistry.enzyme_volume_ul
            + chemistry.part_volume_ul * dna_pieces;
        let water_volume_ul = chemistry
            .reaction_volume_ul
            .checked_sub(consumed)
            .expect("source lowering rejected an over-subscribed reaction");
        assemblies.push(Ot2AssemblyPlan {
            artifact: trace.artifact(context),
            sequence: trace.sequence(context),
            backbone: trace.backbone(context),
            components,
            dependencies: trace.dependencies(context),
            restriction_enzyme: trace.restriction_enzyme(context),
            assembly_replicates,
            water_volume_ul,
            assembly_wells,
            chemistry,
        });
    }
    Ok(assemblies)
}

/// Gives every distinct plasmid carried by any strain in the batch one
/// DNA-plate well.
fn allocate_dna_wells(
    traces: &[StrainTrace],
    context: &Context,
    dna_plate: &Plates,
) -> Result<BTreeMap<String, Ot2Well>, Ot2PlanningError> {
    let carried = traces
        .iter()
        .flat_map(|trace| trace.plasmids(context))
        .collect::<BTreeSet<_>>();
    let mut dna_allocator = PlateAllocator::new("transformation", "dna_plate", dna_plate);
    let dna_wells = dna_allocator.take(carried.len())?;
    Ok(carried.into_iter().zip(dna_wells).collect())
}

/// Builds each strain's transformation and serial-dilution/plating layout,
/// drawing culture wells from the shared reaction plate.
fn build_strain_plans(
    traces: &[StrainTrace],
    context: &Context,
    reaction_plate: &[String],
    dna_source_wells: &BTreeMap<String, Ot2Well>,
    stages: &Stages,
) -> Result<Vec<Ot2StrainPlan>, Ot2PlanningError> {
    let mut culture_cursor = 0;
    let mut dilutions =
        PlateAllocator::new("plating", "dilution_plate", &stages.plating.dilution_plate);
    let mut agar = PlateAllocator::new("plating", "agar_plate", &stages.plating.agar_plate);
    let mut strains = Vec::new();
    for trace in traces {
        let plasmids = trace.plasmids(context);
        let source_wells = plasmids
            .iter()
            .map(|plasmid| {
                dna_source_wells
                    .get(plasmid)
                    .cloned()
                    .expect("every carried plasmid was allocated a DNA-plate well")
            })
            .collect::<Vec<_>>();
        let transformation_replicates = trace.transformation_replicates(context);
        let plating_replicates = trace.plating_replicates(context);
        let serial_dilutions = trace.serial_dilutions(context);

        let mut transformations = Vec::new();
        let mut plating = Vec::new();
        for _ in 0..transformation_replicates {
            if culture_cursor >= reaction_plate.len() {
                return Err(super::constraints::plate_capacity_error(
                    "transformation",
                    "reaction_plate",
                    culture_cursor + 1,
                    reaction_plate.len(),
                ));
            }
            let culture_well = reaction_plate[culture_cursor].clone();
            culture_cursor += 1;
            transformations.push(Ot2TransformationPlan {
                culture_well: culture_well.clone(),
                source_wells: source_wells.clone(),
            });

            let dilution_wells = dilutions.take(usize::from(serial_dilutions))?;
            let mut agar_wells = Vec::new();
            for _ in 0..serial_dilutions {
                agar_wells.push(agar.take(usize::from(plating_replicates))?);
            }
            plating.push(Ot2PlatingPlan {
                culture_well,
                dilution_wells,
                agar_wells,
            });
        }

        strains.push(Ot2StrainPlan {
            artifact: trace.artifact(context),
            host: trace.host(context),
            plasmids,
            dependencies: trace.dependencies(context),
            selection: trace.selection(context),
            transformation_replicates,
            plating_replicates,
            serial_dilutions,
            transformations,
            plating,
            chemistry: Ot2StrainChemistry {
                cell_volume_ul: trace.chemistry(context, "cell_volume_ul"),
                dna_volume_ul: trace.chemistry(context, "dna_volume_ul"),
                recovery_volume_ul: trace.chemistry(context, "recovery_volume_ul"),
                cold_minutes: trace.chemistry(context, "cold_minutes"),
                heat_shock_temperature_c: trace.chemistry(context, "heat_shock_temperature_c"),
                heat_shock_minutes: trace.chemistry(context, "heat_shock_minutes"),
                recovery_temperature_c: trace.chemistry(context, "recovery_temperature_c"),
                recovery_minutes: trace.chemistry(context, "recovery_minutes"),
                medium_volume_ul: trace.chemistry(context, "medium_volume_ul"),
                culture_volume_ul: trace.chemistry(context, "culture_volume_ul"),
                colony_volume_ul: trace.chemistry(context, "colony_volume_ul"),
            },
        });
    }
    Ok(strains)
}

type SourceWells = BTreeMap<String, String>;

/// Assigns source-rack wells for the assembly stage's reagents/DNA/enzymes
/// and the transformation stage's cells/media, sharing one rack.
fn assign_all_source_wells(
    traces: &ProtocolTraces,
    context: &Context,
    rack_capacity: usize,
) -> Result<(SourceWells, SourceWells), Ot2PlanningError> {
    let assembly_source_wells = assign_source_wells(
        "assembly",
        assembly_source_keys(&traces.assemblies, context),
        rack_capacity,
    )?;
    let transformation_source_wells = assign_source_wells(
        "transformation",
        transformation_source_keys(&traces.strains, context),
        rack_capacity,
    )?;
    Ok((assembly_source_wells, transformation_source_wells))
}

#[cfg(test)]
mod tests {
    use lab_language::compile_module;

    use super::*;
    use crate::PortableLairProgram;
    use crate::backend::opentrons_ot2::compile_build;

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
  require topology == circular
  accept sequence == design.sequence

strain reporter_host:
  chassis = DH5alpha
  plasmids = [p_gfp]
  selection = chloramphenicol
  transformation_replicates = 2
  plating_replicates = 2
  serial_dilutions = 2

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

    fn protocol(source: &str) -> crate::ProtocolLairProgram {
        let checked = compile_module(source).unwrap();
        PortableLairProgram::lower(&checked)
            .unwrap()
            .select_protocol()
            .unwrap()
    }

    #[test]
    fn allocates_both_stages_against_the_reference_bench() {
        let protocol = protocol(SOURCE);
        let profile = Ot2TargetProfile::default();
        let plan = plan_build(&protocol, &profile).unwrap();
        let bundle = compile_build(&protocol, &profile).unwrap();

        assert_eq!(bundle.manifest(), &plan);
        assert_eq!(plan.assemblies[0].assembly_wells, ["A1"]);
        assert_eq!(
            plan.strains[0]
                .transformations
                .iter()
                .map(|reaction| reaction.culture_well.as_str())
                .collect::<Vec<_>>(),
            ["A1", "B1"],
            "transformation addresses its own thermocycler plate"
        );
        assert!(
            bundle
                .manual_protocol()
                .contains("Stage 3 — Serial dilution and plating")
        );
        assert_eq!(bundle.artifacts().len(), 5);
    }

    #[test]
    fn source_chemistry_reaches_the_plan_and_rebalances_the_reaction() {
        let tuned = SOURCE.replace(
            "  assembly_replicates = 1\n",
            "  assembly_replicates = 1\n  reaction_volume = 30 uL\n  part_volume = 3 uL\n  assembly_cycles = 40\n",
        );
        let plan = plan_build(&protocol(&tuned), &Ot2TargetProfile::default()).unwrap();
        let chemistry = &plan.assemblies[0].chemistry;

        assert_eq!(chemistry.reaction_volume_ul, 30);
        assert_eq!(chemistry.cycles, 40);
        assert_eq!(
            plan.assemblies[0].water_volume_ul, 7,
            "water makes the reaction up to its stated volume"
        );
    }

    /// A digest runs at the temperature its enzyme works at, so a design that
    /// says nothing about it still cuts correctly.
    #[test]
    fn chemistry_comes_from_the_item_that_owns_it() {
        let owned = SOURCE.replace(
            "buy restriction_enzyme BsaI\n",
            "buy restriction_enzyme BsaI:\n  digest_temperature = 55 C\n  digest_duration = 7 min\n",
        );
        let plan = plan_build(&protocol(&owned), &Ot2TargetProfile::default()).unwrap();
        let chemistry = &plan.assemblies[0].chemistry;

        assert_eq!(chemistry.digest_temperature_c, 55);
        assert_eq!(chemistry.digest_minutes, 7);
    }

    /// A protocol may depart from the datasheet, so a value the design states
    /// wins over the one its enzyme supplies.
    #[test]
    fn a_design_overrides_what_its_enzyme_supplies() {
        let owned = SOURCE
            .replace(
                "buy restriction_enzyme BsaI\n",
                "buy restriction_enzyme BsaI:\n  digest_temperature = 55 C\n",
            )
            .replace(
                "  assembly_replicates = 1\n",
                "  assembly_replicates = 1\n  digest_temperature = 30 C\n",
            );
        let plan = plan_build(&protocol(&owned), &Ot2TargetProfile::default()).unwrap();

        assert_eq!(plan.assemblies[0].chemistry.digest_temperature_c, 30);
    }

    #[test]
    fn rejects_a_reaction_whose_reagents_exceed_its_volume() {
        let over = SOURCE.replace(
            "  assembly_replicates = 1\n",
            "  assembly_replicates = 1\n  reaction_volume = 10 uL\n",
        );
        let checked = compile_module(&over).unwrap();
        let error = PortableLairProgram::lower(&checked)
            .err()
            .expect("an over-subscribed reaction cannot lower");
        assert!(error.to_string().contains("reaction volume"), "{error}");
    }

    /// A method declares the unit each of its quantities is measured in, so a
    /// thousandfold error is caught where it is written rather than when a
    /// target reads it.
    #[test]
    fn rejects_a_chemistry_quantity_in_the_wrong_unit() {
        let wrong = SOURCE.replace(
            "  assembly_replicates = 1\n",
            "  assembly_replicates = 1\n  reaction_volume = 20 mL\n",
        );
        let error = compile_module(&wrong)
            .expect_err("a millilitre reaction volume is not a microlitre one");
        assert!(
            error
                .to_string()
                .contains("expects Quantity<uL>, found Quantity<mL>"),
            "{error}"
        );
    }

    #[test]
    fn a_second_agar_plate_raises_capacity_without_touching_the_program() {
        let crowded = SOURCE.replace("  plating_replicates = 2", "  plating_replicates = 8");
        let protocol = protocol(&crowded);

        let mut single = Ot2TargetProfile::default();
        single.stages.plating.agar_plate.slots = vec!["5".to_owned()];
        single.stages.plating.small_tips.slots = vec!["9".to_owned(), "10".to_owned()];
        let plan = plan_build(&protocol, &single).unwrap();
        assert!(
            plan.strains[0]
                .plating
                .iter()
                .flat_map(|layout| layout.agar_wells.iter().flatten())
                .all(|well| well.plate == 0),
            "one declared plate holds this batch"
        );

        let mut narrow = single.clone();
        narrow.stages.plating.agar_plate.capacity = 24;
        let error =
            plan_build(&protocol, &narrow).expect_err("a 24-well agar plate cannot hold 32 spots");
        assert!(error.to_string().contains("agar_plate"), "{error}");

        let mut spread = narrow.clone();
        spread.stages.plating.agar_plate.slots = vec!["5".to_owned(), "6".to_owned()];
        let plan = plan_build(&protocol, &spread)
            .expect("declaring a second plate accommodates the same program");
        let plates = plan.strains[0]
            .plating
            .iter()
            .flat_map(|layout| layout.agar_wells.iter().flatten())
            .map(|well| well.plate)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            plates,
            BTreeSet::from([0, 1]),
            "allocation spills onto the second plate once the first is full"
        );
    }
}
