use lab_procedure::{
    FluidPathPolicy, Location, MaterialInput, MaterialOutput, PipettingConstraints,
    PipettingProgramV1, PipettingStep, ProcedureProgram, Vessel, VesselRole, Volume,
};

use super::ProcedureTaskInstance;
use super::view::{TaskView, material_symbols, procedure_id};

const MICROLITRE: &str = "http://qudt.org/vocab/unit/MicroL";

pub(super) fn normalize(task: &ProcedureTaskInstance<'_>) -> Result<ProcedureProgram, String> {
    let view = TaskView::new(task);
    view.require_material_roles(&[
        "backbone",
        "components",
        "dependencies",
        "restriction-enzyme",
        "ligase",
        "buffer",
        "water",
    ])?;

    let backbone = view.text_parameter("backbone")?;
    let components = view.text_list_parameter("components")?;
    let dependencies = view.text_list_parameter("dependencies")?;
    let restriction_enzyme = view.text_parameter("restriction_enzyme")?;
    let replicates = view.integer_parameter("assembly_replicates", None)?;
    let reaction_volume = view.integer_parameter("reaction_volume_ul", Some(MICROLITRE))?;
    let part_volume = view.integer_parameter("part_volume_ul", Some(MICROLITRE))?;
    let enzyme_volume = view.integer_parameter("enzyme_volume_ul", Some(MICROLITRE))?;
    let ligase_volume = view.integer_parameter("ligase_volume_ul", Some(MICROLITRE))?;
    let buffer_volume = view.integer_parameter("buffer_volume_ul", Some(MICROLITRE))?;
    let mix_cycles = view.integer_parameter("mix_cycles", None)?;
    let mix_volume = view.integer_parameter("mix_volume_ul", Some(MICROLITRE))?;

    for (name, value) in [
        ("assembly_replicates", replicates),
        ("reaction_volume_ul", reaction_volume),
        ("part_volume_ul", part_volume),
        ("enzyme_volume_ul", enzyme_volume),
        ("ligase_volume_ul", ligase_volume),
        ("buffer_volume_ul", buffer_volume),
        ("mix_cycles", mix_cycles),
        ("mix_volume_ul", mix_volume),
    ] {
        if value == 0 {
            return Err(format!("parameter `{name}` must be greater than zero"));
        }
    }
    if mix_volume > reaction_volume {
        return Err(format!(
            "mix volume {mix_volume} uL exceeds the {reaction_volume} uL reaction volume"
        ));
    }

    let backbone_material = view.one_material("backbone")?;
    if backbone_material.symbol != backbone {
        return Err("parameter `backbone` does not match its material input".to_owned());
    }
    let component_materials = view.materials("components");
    if material_symbols(&component_materials) != components {
        return Err("parameter `components` does not match its material inputs".to_owned());
    }
    let dependency_materials = view.materials("dependencies");
    if material_symbols(&dependency_materials) != dependencies {
        return Err("parameter `dependencies` does not match its material inputs".to_owned());
    }
    let enzyme_material = view.one_material("restriction-enzyme")?;
    if enzyme_material.symbol != restriction_enzyme {
        return Err("parameter `restriction_enzyme` does not match its material input".to_owned());
    }
    let ligase_material = view.one_material("ligase")?;
    let buffer_material = view.one_material("buffer")?;
    let water_material = view.one_material("water")?;

    let dna_piece_count = u32::try_from(component_materials.len() + dependency_materials.len() + 1)
        .map_err(|_| "the reaction contains too many DNA pieces".to_owned())?;
    let consumed = buffer_volume
        .checked_add(ligase_volume)
        .and_then(|value| value.checked_add(enzyme_volume))
        .and_then(|value| value.checked_add(part_volume.checked_mul(dna_piece_count)?))
        .ok_or_else(|| "reaction volume arithmetic overflows".to_owned())?;
    let water_volume = reaction_volume.checked_sub(consumed).ok_or_else(|| {
        format!("reagents require {consumed} uL before water in a {reaction_volume} uL reaction")
    })?;
    let mut additions = Vec::new();
    if water_volume > 0 {
        additions.push((water_material, water_volume));
    }
    additions.extend([
        (buffer_material, buffer_volume),
        (ligase_material, ligase_volume),
        (enzyme_material, enzyme_volume),
        (backbone_material, part_volume),
    ]);
    additions.extend(
        component_materials
            .into_iter()
            .map(|material| (material, part_volume)),
    );
    additions.extend(
        dependency_materials
            .into_iter()
            .map(|material| (material, part_volume)),
    );

    let product = task
        .outputs
        .first()
        .ok_or_else(|| "the task has no material output".to_owned())?;
    if task.outputs.len() != 1 {
        return Err(format!(
            "the pipetting contract requires exactly one product output, found {}",
            task.outputs.len()
        ));
    }
    let product = procedure_id(product.as_str())?;
    let destination_vessel = procedure_id("reaction-plate")?;
    let destinations = (0..replicates)
        .map(|position| Location {
            vessel: destination_vessel.clone(),
            position,
        })
        .collect::<Vec<_>>();

    let mut materials = Vec::with_capacity(additions.len());
    let mut vessels = Vec::with_capacity(additions.len() + 1);
    let mut steps = Vec::with_capacity(additions.len() + 1);
    for (index, (material, volume)) in additions.into_iter().enumerate() {
        let material_id = procedure_id(material.id.as_str())?;
        let source_vessel = procedure_id(&format!("source-{index:04}"))?;
        materials.push(MaterialInput {
            id: material_id.clone(),
        });
        vessels.push(Vessel {
            id: source_vessel.clone(),
            role: VesselRole::MaterialSource {
                material: material_id,
            },
            positions: 1,
        });
        steps.push(PipettingStep::Distribute {
            id: procedure_id(&format!("add-{index:04}"))?,
            source: Location {
                vessel: source_vessel,
                position: 0,
            },
            destinations: destinations.clone(),
            volume_each: Volume::parse_microlitres(volume.to_string())
                .map_err(|error| error.to_string())?,
            fluid_path: FluidPathPolicy::SharedSourceNoReentry,
        });
    }
    vessels.push(Vessel {
        id: destination_vessel,
        role: VesselRole::Product {
            output: product.clone(),
        },
        positions: replicates,
    });
    steps.push(PipettingStep::Mix {
        id: procedure_id("mix-reactions")?,
        targets: destinations,
        cycles: mix_cycles,
        volume: Volume::parse_microlitres(mix_volume.to_string())
            .map_err(|error| error.to_string())?,
        fluid_path: FluidPathPolicy::IsolatedDestinations,
    });

    let program = PipettingProgramV1::new(
        materials,
        vec![MaterialOutput { id: product }],
        vessels,
        steps,
        PipettingConstraints::default(),
    )
    .validate()
    .map_err(|error| error.to_string())?;
    Ok(ProcedureProgram::from_pipetting(&program))
}
