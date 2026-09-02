//! Immutable adapter invocations projected from one exact allocated Procedure program.

use std::collections::{BTreeMap, BTreeSet};

use lab_compiler::allocation::{
    AllocatedMethod, AllocatedProgram, AllocatedProgramExtractionError,
    AllocatedProgramValidationError, InvocationAdapter,
};
use lab_compiler::method::LocalId;
use lab_compiler::planning::MaterialLotInventory;
use lab_compiler::program::AllocatedLairProgram;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const ADAPTER_INVOCATIONS_SCHEMA_VERSION: &str = "lab.adapter-invocations.v1";

/// The complete, immutable backend-facing projection of an allocated Procedure program.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdapterInvocationPlan {
    pub schema_version: String,
    #[serde(flatten)]
    pub allocated: AllocatedProgram,
    pub allocated_lair_sha256: String,
    pub material_inventory: MaterialLotInventory,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invocations: Vec<AdapterInvocation>,
}

/// One exact Asset/adapter invocation. Tasks and requirements refer to the semantic graph above;
/// an adapter never receives unresolved method alternatives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdapterInvocation {
    pub id: String,
    pub asset: String,
    pub adapter: InvocationAdapter,
    pub tasks: Vec<LocalId>,
    pub requirements: Vec<LocalId>,
}

#[derive(Serialize)]
struct CanonicalAdapterInvocationPlan<'a> {
    schema_version: &'a str,
    problem_sha256: &'a str,
    allocated_lair_sha256: &'a str,
    inventory_sha256: &'a str,
    facility: &'a str,
    material_inventory: &'a MaterialLotInventory,
    methods: &'a [AllocatedMethod],
    #[serde(skip_serializing_if = "<[AdapterInvocation]>::is_empty")]
    invocations: &'a [AdapterInvocation],
}

impl AdapterInvocationPlan {
    /// Digest the canonical serde representation consumed by execution scheduling and adapters.
    pub fn sha256(&self) -> String {
        // Preserve the v1 canonical field order even though the owned allocation is serde-flattened.
        let bytes = serde_json::to_vec(&CanonicalAdapterInvocationPlan {
            schema_version: &self.schema_version,
            problem_sha256: &self.allocated.problem_sha256,
            allocated_lair_sha256: &self.allocated_lair_sha256,
            inventory_sha256: &self.allocated.inventory_sha256,
            facility: &self.allocated.facility,
            material_inventory: &self.material_inventory,
            methods: &self.allocated.methods,
            invocations: &self.invocations,
        })
        .expect("AdapterInvocationPlan contains only infallibly serializable semantic values");
        hex_sha256(&bytes)
    }

    /// Project backend invocations from an exact semantic allocation.
    fn from_allocated(
        allocated: AllocatedProgram,
        allocated_lair_sha256: String,
        material_inventory: MaterialLotInventory,
    ) -> Result<Self, AdapterInvocationValidationError> {
        let mut groups = BTreeMap::<(String, InvocationAdapter), InvocationMembers>::new();
        for method in &allocated.methods {
            for task in &method.tasks {
                for requirement in &task.requirements {
                    if let Some(adapter) = &requirement.adapter {
                        let members = groups
                            .entry((requirement.asset.clone(), adapter.clone()))
                            .or_default();
                        members.tasks.insert(task.id.clone());
                        members.requirements.insert(requirement.id.clone());
                    }
                }
            }
        }
        let invocations = groups
            .into_iter()
            .map(|((asset, adapter), members)| AdapterInvocation {
                id: adapter_invocation_id(&asset, &adapter),
                asset,
                adapter,
                tasks: members.tasks.into_iter().collect(),
                requirements: members.requirements.into_iter().collect(),
            })
            .collect();
        let plan = Self {
            schema_version: ADAPTER_INVOCATIONS_SCHEMA_VERSION.to_owned(),
            allocated,
            allocated_lair_sha256,
            material_inventory,
            invocations,
        };
        plan.validate()?;
        Ok(plan)
    }

    /// Project adapter invocations from one exact verifier-valid Allocated LAIR artifact.
    pub fn from_allocated_lair(
        allocated_lair: &AllocatedLairProgram,
        material_inventory: MaterialLotInventory,
    ) -> Result<Self, AdapterInvocationError> {
        let allocated_lair_sha256 = allocated_lair.sha256();
        let allocated = allocated_lair.allocated_program()?;
        Self::from_allocated(allocated, allocated_lair_sha256, material_inventory)
            .map_err(Into::into)
    }

    /// Revalidate a deserialized invocation document before a backend consumes it.
    pub fn validate(&self) -> Result<(), AdapterInvocationValidationError> {
        if self.schema_version != ADAPTER_INVOCATIONS_SCHEMA_VERSION {
            return Err(AdapterInvocationValidationError::WrongSchema {
                found: self.schema_version.clone(),
            });
        }
        if !is_sha256(&self.allocated_lair_sha256) {
            return Err(AdapterInvocationValidationError::InvalidDigest {
                label: "allocated LAIR",
            });
        }
        self.allocated
            .validate_against_material_inventory(&self.material_inventory)?;
        let tasks = self
            .allocated
            .methods
            .iter()
            .flat_map(|method| &method.tasks)
            .map(|task| task.id.clone())
            .collect::<BTreeSet<_>>();
        let requirements = self
            .allocated
            .methods
            .iter()
            .flat_map(|method| &method.tasks)
            .flat_map(|task| {
                task.requirements.iter().map(move |requirement| {
                    (requirement.id.clone(), (task.id.clone(), requirement))
                })
            })
            .collect::<BTreeMap<_, _>>();

        let mut invocation_ids = BTreeSet::new();
        let mut invoked_requirements = BTreeSet::new();
        for invocation in &self.invocations {
            if invocation.id != adapter_invocation_id(&invocation.asset, &invocation.adapter) {
                return Err(AdapterInvocationValidationError::InvalidInvocation {
                    invocation: invocation.id.clone(),
                });
            }
            if !invocation_ids.insert(invocation.id.as_str()) {
                return Err(AdapterInvocationValidationError::DuplicateInvocation {
                    invocation: invocation.id.clone(),
                });
            }
            if invocation.requirements.is_empty() || invocation.tasks.is_empty() {
                return Err(AdapterInvocationValidationError::EmptyInvocation {
                    invocation: invocation.id.clone(),
                });
            }
            let mut invocation_tasks = BTreeSet::new();
            for task in &invocation.tasks {
                if !invocation_tasks.insert(task) || !tasks.contains(task) {
                    return Err(AdapterInvocationValidationError::UnknownTask {
                        invocation: invocation.id.clone(),
                        task: task.clone(),
                    });
                }
            }
            let mut invocation_requirements = BTreeSet::new();
            let mut requirement_owners = BTreeSet::new();
            for requirement_id in &invocation.requirements {
                let Some((task, requirement)) = requirements.get(requirement_id) else {
                    return Err(AdapterInvocationValidationError::UnknownRequirement {
                        invocation: invocation.id.clone(),
                        requirement: requirement_id.clone(),
                    });
                };
                requirement_owners.insert(task);
                if !invocation_requirements.insert(requirement_id)
                    || !invocation.tasks.contains(task)
                    || requirement.asset != invocation.asset
                    || requirement.adapter.as_ref() != Some(&invocation.adapter)
                    || !invoked_requirements.insert(requirement_id.clone())
                {
                    return Err(AdapterInvocationValidationError::InvocationMismatch {
                        invocation: invocation.id.clone(),
                        requirement: requirement_id.clone(),
                    });
                }
            }
            if invocation_tasks != requirement_owners {
                return Err(
                    AdapterInvocationValidationError::InvocationTaskOwnershipMismatch {
                        invocation: invocation.id.clone(),
                    },
                );
            }
        }
        let expected = requirements
            .into_iter()
            .filter_map(|(id, (_, requirement))| requirement.adapter.is_some().then_some(id))
            .collect::<BTreeSet<_>>();
        if invoked_requirements != expected {
            return Err(AdapterInvocationValidationError::InvocationCoverage);
        }
        Ok(())
    }
}

#[derive(Default)]
struct InvocationMembers {
    tasks: BTreeSet<LocalId>,
    requirements: BTreeSet<LocalId>,
}

/// Derive the stable logical ID for an exact Asset and adapter binding.
pub fn adapter_invocation_id(asset: &str, adapter: &InvocationAdapter) -> String {
    let mut identity = Vec::new();
    append_identity_field(&mut identity, asset.as_bytes());
    append_identity_field(&mut identity, adapter.driver.as_bytes());
    append_identity_field(
        &mut identity,
        adapter.profile_path.as_os_str().as_encoded_bytes(),
    );
    append_identity_field(&mut identity, adapter.profile_sha256.as_bytes());
    for values in [
        &adapter.features,
        &adapter.accepted_run_formats,
        &adapter.emitted_run_formats,
    ] {
        append_identity_field(&mut identity, &(values.len() as u64).to_be_bytes());
        for value in values {
            append_identity_field(&mut identity, value.as_bytes());
        }
    }
    let digest = hex_sha256(&identity);
    format!("{}-{}", adapter.driver.replace('.', "-"), &digest[..12])
}

fn append_identity_field(identity: &mut Vec<u8>, field: &[u8]) {
    identity.extend_from_slice(&(field.len() as u64).to_be_bytes());
    identity.extend_from_slice(field);
}

pub(crate) fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Error)]
pub enum AdapterInvocationError {
    #[error(transparent)]
    InvalidAllocatedProgram(#[from] AllocatedProgramExtractionError),
    #[error(transparent)]
    InvalidProjection(#[from] AdapterInvocationValidationError),
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum AdapterInvocationValidationError {
    #[error(
        "adapter invocations declare schema `{found}`, expected `{ADAPTER_INVOCATIONS_SCHEMA_VERSION}`"
    )]
    WrongSchema { found: String },
    #[error("adapter invocations contain an invalid {label} SHA-256 digest")]
    InvalidDigest { label: &'static str },
    #[error(transparent)]
    InvalidAllocatedProgram(#[from] AllocatedProgramValidationError),
    #[error("adapter invocation ID `{invocation}` is empty or repeated")]
    DuplicateInvocation { invocation: String },
    #[error("adapter invocation `{invocation}` does not match its Asset and adapter identity")]
    InvalidInvocation { invocation: String },
    #[error("adapter invocation `{invocation}` contains no tasks or requirements")]
    EmptyInvocation { invocation: String },
    #[error("adapter invocation `{invocation}` references unknown task `{task}`")]
    UnknownTask { invocation: String, task: LocalId },
    #[error("adapter invocation `{invocation}` references unknown requirement `{requirement}`")]
    UnknownRequirement {
        invocation: String,
        requirement: LocalId,
    },
    #[error("adapter invocation `{invocation}` does not match requirement `{requirement}`")]
    InvocationMismatch {
        invocation: String,
        requirement: LocalId,
    },
    #[error("adapter invocation `{invocation}` tasks do not exactly own its requirements")]
    InvocationTaskOwnershipMismatch { invocation: String },
    #[error("adapter invocations do not cover every and only adapter-bound requirement")]
    InvocationCoverage,
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;

    use lab_capability::{CapabilityKind, ControlMode, MethodId, OperationId, QualificationLevel};

    use super::*;
    use lab_compiler::allocation::{
        AllocatedProcedureTask, AllocatedProgram, AllocatedRequirementBinding, InvocationAdapter,
    };
    use lab_compiler::method::{IntentOperationId, PortType};
    use lab_compiler::planning::{
        PlanningMethodYield, PlanningPort, PlanningTaskInput, PlanningTaskOutput,
        PlanningValueSource,
    };

    fn id(value: &str) -> LocalId {
        LocalId::new(value).unwrap()
    }

    fn adapter() -> InvocationAdapter {
        InvocationAdapter {
            driver: "example.driver".to_owned(),
            profile_path: PathBuf::from("profiles/example.toml"),
            profile_sha256: "d".repeat(64),
            features: BTreeSet::from(["temperature-control".to_owned()]),
            accepted_run_formats: BTreeSet::from(["application/json".to_owned()]),
            emitted_run_formats: BTreeSet::from(["text/plain".to_owned()]),
        }
    }

    fn requirement(name: &str, adapter: Option<InvocationAdapter>) -> AllocatedRequirementBinding {
        AllocatedRequirementBinding {
            id: id(name),
            capability_kind: CapabilityKind::new("https://example.org/capability").unwrap(),
            minimum_qualification: QualificationLevel::Executable,
            accepted_control_modes: BTreeSet::from([ControlMode::Manual]),
            offering: format!("https://example.org/offering/{name}"),
            asset: "https://example.org/asset/instrument".to_owned(),
            observed_qualification: QualificationLevel::Executable.to_string(),
            control_mode: ControlMode::Manual.to_string(),
            parameters: Vec::new(),
            procedure_implementation: None,
            adapter,
        }
    }

    fn allocated_program() -> AllocatedProgram {
        let choice = id("choice");
        let task = id("choice::task");
        let input = id("input");
        let output = id("output");
        let task_output = id("task-output");
        AllocatedProgram {
            problem_sha256: "a".repeat(64),
            inventory_sha256: "b".repeat(64),
            facility: "https://example.org/facility".to_owned(),
            methods: vec![AllocatedMethod {
                choice,
                source_operation: IntentOperationId::new("example.operation").unwrap(),
                method: MethodId::new("https://example.org/method").unwrap(),
                after: Vec::new(),
                inputs: vec![PlanningPort {
                    name: input.clone(),
                    port_type: PortType::Design,
                    source: None,
                }],
                outputs: vec![PlanningPort {
                    name: output.clone(),
                    port_type: PortType::Design,
                    source: None,
                }],
                yields: vec![PlanningMethodYield {
                    output,
                    source: PlanningValueSource::TaskOutput {
                        task: task.clone(),
                        output: task_output.clone(),
                    },
                }],
                tasks: vec![AllocatedProcedureTask {
                    id: task,
                    operation: OperationId::new("https://example.org/operation").unwrap(),
                    program: None,
                    inputs: vec![PlanningTaskInput {
                        source: PlanningValueSource::ChoiceInput { input },
                        port_type: PortType::Design,
                    }],
                    outputs: vec![PlanningTaskOutput {
                        name: task_output,
                        port_type: PortType::Design,
                    }],
                    parameters: Vec::new(),
                    materials: Vec::new(),
                    requirements: vec![requirement("choice::requirement", Some(adapter()))],
                }],
            }],
        }
    }

    fn inventory() -> MaterialLotInventory {
        MaterialLotInventory::new(
            "b".repeat(64),
            "https://example.org/facility",
            BTreeMap::new(),
            BTreeMap::new(),
        )
    }

    fn valid_plan() -> AdapterInvocationPlan {
        AdapterInvocationPlan::from_allocated(allocated_program(), "c".repeat(64), inventory())
            .unwrap()
    }

    #[test]
    fn invocation_tasks_are_exactly_the_requirement_owners() {
        let mut plan = valid_plan();
        let extra = id("choice::manual-task");
        plan.allocated.methods[0]
            .tasks
            .push(AllocatedProcedureTask {
                id: extra.clone(),
                operation: OperationId::new("https://example.org/manual-operation").unwrap(),
                program: None,
                inputs: Vec::new(),
                outputs: Vec::new(),
                parameters: Vec::new(),
                materials: Vec::new(),
                requirements: vec![requirement("choice::manual-requirement", None)],
            });
        plan.invocations[0].tasks.push(extra);
        assert!(matches!(
            plan.validate(),
            Err(AdapterInvocationValidationError::InvocationTaskOwnershipMismatch { .. })
        ));
    }

    #[test]
    fn invocation_identity_covers_every_adapter_grouping_field() {
        let original = adapter();
        let original_id = adapter_invocation_id("https://example.org/asset", &original);
        for changed in [
            {
                let mut changed = original.clone();
                changed.features.insert("new-feature".to_owned());
                changed
            },
            {
                let mut changed = original.clone();
                changed
                    .accepted_run_formats
                    .insert("application/xml".to_owned());
                changed
            },
            {
                let mut changed = original.clone();
                changed
                    .emitted_run_formats
                    .insert("application/yaml".to_owned());
                changed
            },
        ] {
            assert_ne!(
                adapter_invocation_id("https://example.org/asset", &changed),
                original_id
            );
        }
    }

    #[test]
    fn flattened_allocation_preserves_the_v1_json_shape_and_hash_contract() {
        let plan = valid_plan();
        let encoded = serde_json::to_value(&plan).unwrap();
        let object = encoded.as_object().unwrap();
        assert!(!object.contains_key("allocated"));
        assert_eq!(
            object.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "allocated_lair_sha256",
                "facility",
                "inventory_sha256",
                "invocations",
                "material_inventory",
                "methods",
                "problem_sha256",
                "schema_version",
            ])
        );

        let legacy = CanonicalAdapterInvocationPlan {
            schema_version: &plan.schema_version,
            problem_sha256: &plan.allocated.problem_sha256,
            allocated_lair_sha256: &plan.allocated_lair_sha256,
            inventory_sha256: &plan.allocated.inventory_sha256,
            facility: &plan.allocated.facility,
            material_inventory: &plan.material_inventory,
            methods: &plan.allocated.methods,
            invocations: &plan.invocations,
        };
        assert_eq!(encoded, serde_json::to_value(&legacy).unwrap());
        assert_eq!(
            plan.sha256(),
            hex_sha256(&serde_json::to_vec(&legacy).unwrap())
        );
        assert_eq!(
            serde_json::from_value::<AdapterInvocationPlan>(encoded).unwrap(),
            plan
        );

        let schema = serde_json::to_value(schemars::schema_for!(AdapterInvocationPlan)).unwrap();
        let properties = schema["properties"].as_object().unwrap();
        assert!(!properties.contains_key("allocated"));
        for property in ["problem_sha256", "inventory_sha256", "facility", "methods"] {
            assert!(
                properties.contains_key(property),
                "missing `{property}` schema"
            );
        }
    }

    #[test]
    fn manual_only_plan_preserves_the_v1_omitted_invocations_hash_contract() {
        let mut allocated = allocated_program();
        allocated.methods[0].tasks[0].requirements[0].adapter = None;
        let plan =
            AdapterInvocationPlan::from_allocated(allocated, "c".repeat(64), inventory()).unwrap();
        assert!(plan.invocations.is_empty());

        let legacy = CanonicalAdapterInvocationPlan {
            schema_version: &plan.schema_version,
            problem_sha256: &plan.allocated.problem_sha256,
            allocated_lair_sha256: &plan.allocated_lair_sha256,
            inventory_sha256: &plan.allocated.inventory_sha256,
            facility: &plan.allocated.facility,
            material_inventory: &plan.material_inventory,
            methods: &plan.allocated.methods,
            invocations: &plan.invocations,
        };
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        assert!(
            !String::from_utf8(legacy_bytes.clone())
                .unwrap()
                .contains("\"invocations\"")
        );
        assert_eq!(plan.sha256(), hex_sha256(&legacy_bytes));
    }
}
