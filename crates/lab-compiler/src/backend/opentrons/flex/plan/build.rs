//! Construction of the validated, resource-allocated Flex execution plan.

use std::collections::{BTreeMap, BTreeSet};

use pliron::context::Context;

use crate::ProtocolLairProgram;
use crate::backend::opentrons::flex::profile::{FlexAdapterProfile, Plates, Stages};

use crate::backend::opentrons::flex::BACKEND;
use crate::backend::opentrons::flex::plan::constraints::{
    require_tip_capacity, validate_assembly_constraints, validate_strain_constraints,
    validate_uniform_batch_settings,
};
use crate::backend::opentrons::flex::plan::{
    FlexAssemblyChemistry, FlexAssemblyPlan, FlexExecutionPlan, FlexPlanningError, FlexPlatingPlan,
    FlexStrainChemistry, FlexStrainPlan, FlexTransformationPlan, FlexWell,
};
use crate::backend::resources::{
    PlateAllocator, assembly_source_keys, assign_source_wells, plate_wells, require_known_geometry,
    transformation_source_keys,
};
use crate::backend::trace::{AssemblyTrace, ProtocolTraces, StrainTrace, analyze_protocol};

/// Validate and allocate a Flex build against one bench without rendering any
/// output files.
///
/// The returned execution plan is the single input shared by the JSON and
/// Markdown emitters.
pub fn plan_build(
    protocol: &ProtocolLairProgram,
    profile: &FlexAdapterProfile,
) -> Result<FlexExecutionPlan, FlexPlanningError> {
    plan_selected_build(protocol, profile, None)
}

pub(in crate::backend::opentrons::flex) fn plan_selected_build(
    protocol: &ProtocolLairProgram,
    profile: &FlexAdapterProfile,
    selected_artifacts: Option<&BTreeSet<String>>,
) -> Result<FlexExecutionPlan, FlexPlanningError> {
    let context = protocol.context();
    let traces = analyze_protocol(protocol, selected_artifacts)?;
    validate_traces(&traces, context)?;

    let deck = &profile.deck;
    let stages = &profile.stages;
    require_deck_geometry(profile)?;
    require_stage_tip_capacities(&traces, context, stages)?;

    // The thermocycler holds one plate, so assembly and transformation each
    // address it as a single plate rather than a spillable stage resource.
    let reaction_plate = plate_wells(deck.thermocycler.capacity);

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

    Ok(FlexExecutionPlan {
        schema_version: "lab.automation.v1".into(),
        adapter: BACKEND.into(),
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
fn validate_traces(traces: &ProtocolTraces, context: &Context) -> Result<(), FlexPlanningError> {
    for trace in &traces.assemblies {
        validate_assembly_constraints(trace, context)?;
    }
    for trace in &traces.strains {
        validate_strain_constraints(trace, context)?;
    }
    validate_uniform_batch_settings(&traces.strains, context)
}

/// Rejects adapter configuration that declares labware in a well count this implementation has no row/column layout for. The tip racks are included because the JSON emitter addresses tip wells by name.
fn require_deck_geometry(profile: &FlexAdapterProfile) -> Result<(), FlexPlanningError> {
    let deck = &profile.deck;
    let stages = &profile.stages;
    require_known_geometry("the source rack", deck.temperature_module.capacity)?;
    require_known_geometry("the reaction plate", deck.thermocycler.capacity)?;
    require_known_geometry("the DNA plate", stages.transformation.dna_plate.capacity)?;
    require_known_geometry("the dilution plate", stages.plating.dilution_plate.capacity)?;
    require_known_geometry("the agar plate", stages.plating.agar_plate.capacity)?;
    for (resource, capacity) in [
        ("the assembly tip rack", stages.assembly.small_tips.capacity),
        (
            "the transformation small tip rack",
            stages.transformation.small_tips.capacity,
        ),
        (
            "the transformation large tip rack",
            stages.transformation.large_tips.capacity,
        ),
        (
            "the plating small tip rack",
            stages.plating.small_tips.capacity,
        ),
        (
            "the plating large tip rack",
            stages.plating.large_tips.capacity,
        ),
    ] {
        require_known_geometry(resource, capacity)?;
    }
    Ok(())
}

/// Checks the small/large tip racks declared for each stage against the tip
/// count that stage's reactions will consume.
fn require_stage_tip_capacities(
    traces: &ProtocolTraces,
    context: &Context,
    stages: &Stages,
) -> Result<(), FlexPlanningError> {
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
) -> Result<Vec<FlexAssemblyPlan>, FlexPlanningError> {
    let mut assembly_cursor = 0;
    let mut assemblies = Vec::new();
    for trace in traces {
        let assembly_replicates = trace.assembly_replicates(context);
        let end = assembly_cursor + usize::from(assembly_replicates);
        if end > reaction_plate.len() {
            return Err(
                crate::backend::opentrons::flex::plan::constraints::plate_capacity_error(
                    "assembly",
                    "reaction_plate",
                    end,
                    reaction_plate.len(),
                ),
            );
        }
        let assembly_wells = reaction_plate[assembly_cursor..end].to_vec();
        assembly_cursor = end;
        let components = trace.components(context);
        let chemistry = FlexAssemblyChemistry {
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
        assemblies.push(FlexAssemblyPlan {
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
) -> Result<BTreeMap<String, FlexWell>, FlexPlanningError> {
    let carried = traces
        .iter()
        .flat_map(|trace| trace.plasmids(context))
        .collect::<BTreeSet<_>>();
    let mut dna_allocator = PlateAllocator::new(BACKEND, "transformation", "dna_plate", dna_plate);
    let dna_wells = dna_allocator.take(carried.len())?;
    Ok(carried.into_iter().zip(dna_wells).collect())
}

/// Builds each strain's transformation and serial-dilution/plating layout,
/// drawing culture wells from the shared reaction plate.
fn build_strain_plans(
    traces: &[StrainTrace],
    context: &Context,
    reaction_plate: &[String],
    dna_source_wells: &BTreeMap<String, FlexWell>,
    stages: &Stages,
) -> Result<Vec<FlexStrainPlan>, FlexPlanningError> {
    let mut culture_cursor = 0;
    let mut dilutions = PlateAllocator::new(
        BACKEND,
        "plating",
        "dilution_plate",
        &stages.plating.dilution_plate,
    );
    let mut agar =
        PlateAllocator::new(BACKEND, "plating", "agar_plate", &stages.plating.agar_plate);
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
                return Err(
                    crate::backend::opentrons::flex::plan::constraints::plate_capacity_error(
                        "transformation",
                        "reaction_plate",
                        culture_cursor + 1,
                        reaction_plate.len(),
                    ),
                );
            }
            let culture_well = reaction_plate[culture_cursor].clone();
            culture_cursor += 1;
            transformations.push(FlexTransformationPlan {
                culture_well: culture_well.clone(),
                source_wells: source_wells.clone(),
            });

            let dilution_wells = dilutions.take(usize::from(serial_dilutions))?;
            let mut agar_wells = Vec::new();
            for _ in 0..serial_dilutions {
                agar_wells.push(agar.take(usize::from(plating_replicates))?);
            }
            plating.push(FlexPlatingPlan {
                culture_well,
                dilution_wells,
                agar_wells,
            });
        }

        strains.push(FlexStrainPlan {
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
            chemistry: FlexStrainChemistry {
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
) -> Result<(SourceWells, SourceWells), FlexPlanningError> {
    let assembly_source_wells = assign_source_wells(
        BACKEND,
        "assembly",
        assembly_source_keys(&traces.assemblies, context),
        rack_capacity,
    )?;
    let transformation_source_wells = assign_source_wells(
        BACKEND,
        "transformation",
        transformation_source_keys(&traces.strains, context),
        rack_capacity,
    )?;
    Ok((assembly_source_wells, transformation_source_wells))
}

#[cfg(test)]
mod tests {
    use crate::backend::opentrons::flex::plan::build::*;
    use crate::backend::opentrons::flex::profile::FlexAdapterProfile;
    use crate::test_support::golden_gate_protocol;

    #[test]
    fn allocates_both_stages_against_the_reference_bench() {
        let protocol = golden_gate_protocol();
        let plan = plan_build(&protocol, &FlexAdapterProfile::default()).unwrap();
        assert_eq!(plan.adapter, "opentrons.flex");
        assert_eq!(plan.schema_version, "lab.automation.v1");
        assert_eq!(plan.assemblies.len(), 2);
        assert_eq!(plan.strains.len(), 4);
        assert_eq!(plan.assemblies[0].assembly_wells, ["A1"]);
        assert!(
            plan.assembly_source_wells
                .contains_key("reagent:nuclease_free_water"),
            "assembly reagents share the chilled source rack"
        );
    }

    #[test]
    fn a_narrow_agar_plate_is_a_capacity_error_naming_this_backend() {
        let protocol = golden_gate_protocol();
        let mut narrow = FlexAdapterProfile::default();
        narrow.stages.plating.agar_plate.slots = vec!["B2".to_owned()];
        narrow.stages.plating.agar_plate.capacity = 15;
        let error = plan_build(&protocol, &narrow)
            .expect_err("one 15-well agar plate cannot hold this batch's 16 spots");
        let message = error.to_string();
        assert!(message.contains("agar_plate"), "{message}");
        assert!(message.contains("opentrons.flex"), "{message}");
    }
}
