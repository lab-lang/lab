//! Durable, target-neutral facility-planning contracts projected from LAIR.

mod extraction;
mod material_inventory;
mod problem;
mod solution;

pub use extraction::PlanningProblemExtractionError;
pub(crate) use extraction::extract_planning_problem;
pub use material_inventory::{
    MaterialLotCandidates, MaterialLotInventory, MaterialLotInventoryValidationError,
};
pub use problem::{
    PLANNING_PROBLEM_SCHEMA_VERSION, PlanningCapabilityRequirement, PlanningMaterialInput,
    PlanningMaterialSource, PlanningMethodCandidate, PlanningMethodChoice, PlanningMethodYield,
    PlanningPort, PlanningProblem, PlanningProblemValidationError, PlanningProcedureParameter,
    PlanningProcedureTask, PlanningTaskInput, PlanningTaskOutput, PlanningValueSource,
};
pub use solution::{
    AdapterRequirement, AssetPin, AssetPinSelector, FACILITY_PLANNING_SOLUTION_SCHEMA_VERSION,
    FacilityPlanningPolicy, FacilityPlanningSolution, FacilityPlanningSolutionValidationError,
    MethodPin, MethodPinSelector, PlanningCandidateRejectionReason, PlanningRejectedOffering,
    SelectedAdapter, SelectedCapabilityParameter, SelectedMaterialBinding, SelectedMaterialSource,
    SelectedMethod, SelectedProcedureTask, SelectedRequirementBinding,
};
