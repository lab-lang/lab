//! Lower canonical thermal programs into reviewed Inheco ODTC run documents.

use lab_runfmt::{THERMOCYCLE_RUN_FORMAT, ThermocycleRunDocument};

use crate::backend::adapters::{AdapterInvocationDocument, AdapterInvocationLowering};
use crate::backend::invocation::exact_invocation_tasks;
use crate::backend::procedure::{CYCLE_GOLDEN_GATE, normalized_thermal_program};
use crate::planning::{AdapterInvocation, AdapterInvocationPlan, PlanningValueSource};
use crate::{ArtifactBundle, GeneratedArtifact};

pub(in crate::backend) fn lower_invocation(
    invocation_plan: &AdapterInvocationPlan,
    invocation: &AdapterInvocation,
) -> Result<AdapterInvocationLowering, String> {
    let tasks = exact_invocation_tasks("Inheco ODTC", invocation_plan, invocation)?;
    let mut artifacts = ArtifactBundle::new();
    let mut documents = Vec::new();
    for (ordinal, member) in tasks.into_iter().enumerate() {
        if member.task.operation.as_str() != CYCLE_GOLDEN_GATE {
            return Err(format!(
                "Inheco ODTC invocation contains unsupported Procedure operation '{}' in task '{}'",
                member.task.operation, member.task.id
            ));
        }
        let program = normalized_thermal_program("Inheco ODTC", member.task, &member.requirements)?;
        program
            .profile
            .validate(&lab_instruments::odtc_thermal_limits())
            .map_err(|error| {
                format!(
                    "Inheco ODTC Procedure task '{}' is outside the device envelope: {error}",
                    member.task.id
                )
            })?;
        let input = member
            .task
            .inputs
            .first()
            .expect("validated thermal task has its exact input binding");
        let document = ThermocycleRunDocument {
            format: THERMOCYCLE_RUN_FORMAT.to_owned(),
            id: member.task.id.to_string(),
            title: format!("Thermal cycle {}", program.artifact),
            plate: render_value_source(&input.source),
            profile: program.profile,
            final_hold_celsius: program.final_hold_celsius,
            fill_volume_ul: program.volume_each_ul,
        };
        let path = format!("tasks/{:03}-thermal-program/thermocycle.json", ordinal + 1);
        let mut contents = serde_json::to_string_pretty(&document).map_err(|error| {
            format!(
                "could not serialize Inheco ODTC Procedure task '{}': {error}",
                member.task.id
            )
        })?;
        contents.push('\n');
        artifacts
            .insert(
                GeneratedArtifact::text(&path, "application/json", contents)
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        documents.push(AdapterInvocationDocument {
            requirements: member
                .requirements
                .iter()
                .map(|requirement| requirement.id.clone())
                .collect(),
            path,
            format: THERMOCYCLE_RUN_FORMAT.to_owned(),
        });
    }
    Ok(AdapterInvocationLowering {
        artifacts,
        documents,
    })
}

fn render_value_source(source: &PlanningValueSource) -> String {
    match source {
        PlanningValueSource::ChoiceInput { input } => format!("choice input {input}"),
        PlanningValueSource::ChoiceOutput { choice, output } => {
            format!("Method choice {choice} output {output}")
        }
        PlanningValueSource::TaskOutput { task, output } => {
            format!("Procedure task {task} output {output}")
        }
    }
}
