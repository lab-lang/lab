//! Exact facility decisions attached to selected Procedure and Capability operations.

use crate::method::{IntentOperationId, LocalId};
use lab_capability::{AbsoluteIri, ControlMode, MethodId, QualificationLevel};
use pliron::builtin::attributes::{StringAttr, VecAttr};
use pliron::builtin::op_interfaces::{NOpdsInterface, NResultsInterface};
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::pliron_op;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::verify_err;

use crate::lair::dialect::attributes::string_vec;
use crate::lair::dialect::procedure::is_stable_local_id;
use crate::planning::{
    SelectedMaterialBinding, SelectedMaterialSource, SelectedMethod, SelectedRequirementBinding,
};

/// Binds allocated LAIR to the exact immutable planning and facility inputs.
#[pliron_op(
    name = "allocation.context",
    format,
    attributes = (
        problem_sha256: StringAttr,
        inventory_sha256: StringAttr,
        facility: StringAttr
    ),
    interfaces = [NOpdsInterface<0>, NResultsInterface<0>]
)]
pub(crate) struct ContextOp;

impl ContextOp {
    pub(crate) fn new(
        context: &mut Context,
        problem_sha256: impl Into<String>,
        inventory_sha256: impl Into<String>,
        facility: impl Into<String>,
    ) -> Self {
        let raw = Operation::new(
            context,
            Self::get_concrete_op_info(),
            vec![],
            vec![],
            vec![],
            0,
        );
        let result = Self { op: raw };
        result.set_attr_problem_sha256(context, StringAttr::new(problem_sha256.into()));
        result.set_attr_inventory_sha256(context, StringAttr::new(inventory_sha256.into()));
        result.set_attr_facility(context, StringAttr::new(facility.into()));
        result
    }
}

impl Verify for ContextOp {
    fn verify(&self, context: &Context) -> Result<()> {
        for (name, value) in [
            ("problem_sha256", self.get_attr_problem_sha256(context)),
            ("inventory_sha256", self.get_attr_inventory_sha256(context)),
        ] {
            if value.is_none_or(|value| !is_sha256(value.as_str())) {
                return verify_err!(
                    self.loc(context),
                    "allocation.context {name} must be a lowercase SHA-256 digest"
                );
            }
        }
        if self
            .get_attr_facility(context)
            .is_none_or(|value| AbsoluteIri::new(value.as_str()).is_err())
        {
            return verify_err!(
                self.loc(context),
                "allocation.context facility must be an absolute IRI"
            );
        }
        Ok(())
    }
}

/// Records the exact Method selected for one former `method.choice`.
#[pliron_op(
    name = "allocation.method",
    format,
    attributes = (
        selected_choice: StringAttr,
        selected_source_operation: StringAttr,
        selected_method: StringAttr,
        selected_procedure_nodes: VecAttr
    ),
    interfaces = [NOpdsInterface<0>, NResultsInterface<0>]
)]
pub(crate) struct MethodOp;

impl MethodOp {
    pub(crate) fn new(context: &mut Context, selection: &SelectedMethod) -> Self {
        let raw = Operation::new(
            context,
            Self::get_concrete_op_info(),
            vec![],
            vec![],
            vec![],
            0,
        );
        let result = Self { op: raw };
        result.set_attr_selected_choice(context, StringAttr::new(selection.choice.to_string()));
        result.set_attr_selected_source_operation(
            context,
            StringAttr::new(selection.source_operation.to_string()),
        );
        result.set_attr_selected_method(context, StringAttr::new(selection.method.to_string()));
        result.set_attr_selected_procedure_nodes(
            context,
            string_vec(
                selection
                    .tasks
                    .iter()
                    .map(|task| task.task.to_string())
                    .collect(),
            ),
        );
        result
    }

    pub(crate) fn choice(&self, context: &Context) -> LocalId {
        LocalId::new(
            self.get_attr_selected_choice(context)
                .expect("verified allocation.method carries a choice")
                .as_str(),
        )
        .expect("verified allocation.method choice is stable")
    }

    pub(crate) fn procedure_nodes(&self, context: &Context) -> Vec<LocalId> {
        self.get_attr_selected_procedure_nodes(context)
            .expect("verified allocation.method carries Procedure nodes")
            .0
            .iter()
            .map(|value| {
                LocalId::new(
                    value
                        .downcast_ref::<StringAttr>()
                        .expect("verified Procedure node identities are strings")
                        .as_str(),
                )
                .expect("verified Procedure node identity is stable")
            })
            .collect()
    }
}

impl Verify for MethodOp {
    fn verify(&self, context: &Context) -> Result<()> {
        if self
            .get_attr_selected_choice(context)
            .is_none_or(|value| LocalId::new(value.as_str()).is_err())
        {
            return verify_err!(
                self.loc(context),
                "allocation.method selected_choice must be a stable local ID"
            );
        }
        if self
            .get_attr_selected_source_operation(context)
            .is_none_or(|value| IntentOperationId::new(value.as_str()).is_err())
        {
            return verify_err!(
                self.loc(context),
                "allocation.method selected_source_operation must be a stable operation ID"
            );
        }
        if self
            .get_attr_selected_method(context)
            .is_none_or(|value| MethodId::new(value.as_str()).is_err())
        {
            return verify_err!(
                self.loc(context),
                "allocation.method selected_method must be an absolute IRI"
            );
        }
        let Some(nodes) = self.get_attr_selected_procedure_nodes(context) else {
            return verify_err!(
                self.loc(context),
                "allocation.method is missing selected_procedure_nodes"
            );
        };
        if nodes.0.is_empty() {
            return verify_err!(
                self.loc(context),
                "allocation.method must select at least one Procedure node"
            );
        }
        let mut seen = std::collections::BTreeSet::new();
        for node in &nodes.0 {
            let Some(node) = node.downcast_ref::<StringAttr>() else {
                return verify_err!(
                    self.loc(context),
                    "allocation.method Procedure nodes must be strings"
                );
            };
            if !is_stable_local_id(node.as_str()) || !seen.insert(node.as_str()) {
                return verify_err!(
                    self.loc(context),
                    "allocation.method Procedure nodes must be stable and unique"
                );
            }
        }
        Ok(())
    }
}

/// Freezes the physical source selected for one Procedure material input.
#[pliron_op(
    name = "allocation.material",
    format,
    attributes = (
        bound_material_input: StringAttr,
        material_procedure_node: StringAttr,
        bound_material_symbol: StringAttr,
        material_source_kind: StringAttr,
        bound_component: StringAttr,
        bound_material_lot: StringAttr,
        bound_choice: StringAttr
    ),
    interfaces = [NOpdsInterface<0>, NResultsInterface<0>]
)]
pub(crate) struct MaterialBindingOp;

impl MaterialBindingOp {
    pub(crate) fn new(
        context: &mut Context,
        procedure_node: &LocalId,
        binding: &SelectedMaterialBinding,
    ) -> Self {
        let raw = Operation::new(
            context,
            Self::get_concrete_op_info(),
            vec![],
            vec![],
            vec![],
            0,
        );
        let result = Self { op: raw };
        result.set_attr_bound_material_input(context, StringAttr::new(binding.input.to_string()));
        result
            .set_attr_material_procedure_node(context, StringAttr::new(procedure_node.to_string()));
        result.set_attr_bound_material_symbol(context, StringAttr::new(binding.symbol.clone()));
        match &binding.source {
            SelectedMaterialSource::MaterialLot {
                component,
                material_lot,
            } => {
                result.set_attr_material_source_kind(
                    context,
                    StringAttr::new("material_lot".to_owned()),
                );
                result.set_attr_bound_component(context, StringAttr::new(component.clone()));
                result.set_attr_bound_material_lot(context, StringAttr::new(material_lot.clone()));
            }
            SelectedMaterialSource::ChoiceOutput { choice } => {
                result.set_attr_material_source_kind(
                    context,
                    StringAttr::new("choice_output".to_owned()),
                );
                result.set_attr_bound_choice(context, StringAttr::new(choice.to_string()));
            }
        }
        result
    }

    pub(crate) fn input(&self, context: &Context) -> LocalId {
        LocalId::new(
            self.get_attr_bound_material_input(context)
                .expect("verified allocation.material carries an input")
                .as_str(),
        )
        .expect("verified allocation.material input is stable")
    }

    pub(crate) fn procedure_node(&self, context: &Context) -> LocalId {
        LocalId::new(
            self.get_attr_material_procedure_node(context)
                .expect("verified allocation.material carries a Procedure node")
                .as_str(),
        )
        .expect("verified allocation.material Procedure node is stable")
    }

    pub(crate) fn symbol(&self, context: &Context) -> String {
        self.get_attr_bound_material_symbol(context)
            .expect("verified allocation.material carries a symbol")
            .as_str()
            .to_owned()
    }
}

impl Verify for MaterialBindingOp {
    fn verify(&self, context: &Context) -> Result<()> {
        for (name, value) in [
            (
                "bound_material_input",
                self.get_attr_bound_material_input(context),
            ),
            (
                "material_procedure_node",
                self.get_attr_material_procedure_node(context),
            ),
        ] {
            if value.is_none_or(|value| LocalId::new(value.as_str()).is_err()) {
                return verify_err!(
                    self.loc(context),
                    "allocation.material {name} must be a stable local ID"
                );
            }
        }
        if self
            .get_attr_bound_material_symbol(context)
            .is_none_or(|value| value.as_str().is_empty())
        {
            return verify_err!(
                self.loc(context),
                "allocation.material symbol must be non-empty"
            );
        }
        let component = self.get_attr_bound_component(context);
        let material_lot = self.get_attr_bound_material_lot(context);
        let choice = self.get_attr_bound_choice(context);
        match self
            .get_attr_material_source_kind(context)
            .as_deref()
            .map(StringAttr::as_str)
        {
            Some("material_lot")
                if component
                    .as_ref()
                    .is_some_and(|value| AbsoluteIri::new(value.as_str()).is_ok())
                    && material_lot
                        .as_ref()
                        .is_some_and(|value| AbsoluteIri::new(value.as_str()).is_ok())
                    && choice.is_none() =>
            {
                Ok(())
            }
            Some("choice_output")
                if component.is_none()
                    && material_lot.is_none()
                    && choice.is_some_and(|value| LocalId::new(value.as_str()).is_ok()) =>
            {
                Ok(())
            }
            _ => verify_err!(
                self.loc(context),
                "allocation.material must carry exactly one valid material_lot or choice_output source"
            ),
        }
    }
}

/// Freezes one exact Requirement-to-offering-to-Asset binding and optional implementation.
#[pliron_op(
    name = "allocation.binding",
    format,
    attributes = (
        bound_requirement: StringAttr,
        bound_procedure_node: StringAttr,
        bound_offering: StringAttr,
        bound_asset: StringAttr,
        observed_qualification: StringAttr,
        observed_control_mode: StringAttr,
        matched_offering_parameters: VecAttr,
        adapter_driver: StringAttr,
        adapter_profile_path: StringAttr,
        adapter_profile_sha256: StringAttr,
        adapter_accepted_run_formats: VecAttr,
        adapter_emitted_run_formats: VecAttr
    ),
    interfaces = [NOpdsInterface<0>, NResultsInterface<0>]
)]
pub(crate) struct BindingOp;

impl BindingOp {
    pub(crate) fn new(
        context: &mut Context,
        procedure_node: &LocalId,
        binding: &SelectedRequirementBinding,
    ) -> Self {
        let raw = Operation::new(
            context,
            Self::get_concrete_op_info(),
            vec![],
            vec![],
            vec![],
            0,
        );
        let result = Self { op: raw };
        result
            .set_attr_bound_requirement(context, StringAttr::new(binding.requirement.to_string()));
        result.set_attr_bound_procedure_node(context, StringAttr::new(procedure_node.to_string()));
        result.set_attr_bound_offering(context, StringAttr::new(binding.offering.clone()));
        result.set_attr_bound_asset(context, StringAttr::new(binding.asset.clone()));
        result.set_attr_observed_qualification(
            context,
            StringAttr::new(binding.observed_qualification.clone()),
        );
        result
            .set_attr_observed_control_mode(context, StringAttr::new(binding.control_mode.clone()));
        result.set_attr_matched_offering_parameters(
            context,
            string_vec(
                binding
                    .parameters
                    .iter()
                    .map(|parameter| parameter.offering_parameter.clone())
                    .collect(),
            ),
        );
        if let Some(adapter) = &binding.adapter {
            result.set_attr_adapter_driver(context, StringAttr::new(adapter.driver.clone()));
            result.set_attr_adapter_profile_path(
                context,
                StringAttr::new(adapter.profile_path.to_string_lossy().into_owned()),
            );
            result.set_attr_adapter_profile_sha256(
                context,
                StringAttr::new(adapter.profile_sha256.clone()),
            );
            result.set_attr_adapter_accepted_run_formats(
                context,
                string_vec(adapter.accepted_run_formats.iter().cloned().collect()),
            );
            result.set_attr_adapter_emitted_run_formats(
                context,
                string_vec(adapter.emitted_run_formats.iter().cloned().collect()),
            );
        }
        result
    }

    pub(crate) fn requirement(&self, context: &Context) -> LocalId {
        LocalId::new(
            self.get_attr_bound_requirement(context)
                .expect("verified allocation.binding carries a Requirement")
                .as_str(),
        )
        .expect("verified allocation.binding Requirement is stable")
    }

    pub(crate) fn procedure_node(&self, context: &Context) -> LocalId {
        LocalId::new(
            self.get_attr_bound_procedure_node(context)
                .expect("verified allocation.binding carries a Procedure node")
                .as_str(),
        )
        .expect("verified allocation.binding Procedure node is stable")
    }
}

impl Verify for BindingOp {
    fn verify(&self, context: &Context) -> Result<()> {
        for (name, value) in [
            (
                "bound_requirement",
                self.get_attr_bound_requirement(context),
            ),
            (
                "bound_procedure_node",
                self.get_attr_bound_procedure_node(context),
            ),
        ] {
            if value.is_none_or(|value| LocalId::new(value.as_str()).is_err()) {
                return verify_err!(
                    self.loc(context),
                    "allocation.binding {name} must be a stable local ID"
                );
            }
        }
        for (name, value) in [
            ("bound_offering", self.get_attr_bound_offering(context)),
            ("bound_asset", self.get_attr_bound_asset(context)),
        ] {
            if value.is_none_or(|value| AbsoluteIri::new(value.as_str()).is_err()) {
                return verify_err!(
                    self.loc(context),
                    "allocation.binding {name} must be an absolute IRI"
                );
            }
        }
        if self
            .get_attr_observed_qualification(context)
            .is_none_or(|value| QualificationLevel::try_from(value.as_str()).is_err())
        {
            return verify_err!(
                self.loc(context),
                "allocation.binding observed_qualification must use the closed vocabulary"
            );
        }
        if self
            .get_attr_observed_control_mode(context)
            .is_none_or(|value| ControlMode::try_from(value.as_str()).is_err())
        {
            return verify_err!(
                self.loc(context),
                "allocation.binding observed_control_mode must use the closed vocabulary"
            );
        }
        let Some(parameters) = self.get_attr_matched_offering_parameters(context) else {
            return verify_err!(
                self.loc(context),
                "allocation.binding is missing matched_offering_parameters"
            );
        };
        for parameter in &parameters.0 {
            if parameter
                .downcast_ref::<StringAttr>()
                .is_none_or(|value| AbsoluteIri::new(value.as_str()).is_err())
            {
                return verify_err!(
                    self.loc(context),
                    "allocation.binding matched parameters must be absolute IRIs"
                );
            }
        }
        self.verify_adapter(context)
    }
}

impl BindingOp {
    fn verify_adapter(&self, context: &Context) -> Result<()> {
        let driver = self.get_attr_adapter_driver(context);
        let path = self.get_attr_adapter_profile_path(context);
        let digest = self.get_attr_adapter_profile_sha256(context);
        let accepted = self.get_attr_adapter_accepted_run_formats(context);
        let emitted = self.get_attr_adapter_emitted_run_formats(context);
        let present = [
            driver.is_some(),
            path.is_some(),
            digest.is_some(),
            accepted.is_some(),
            emitted.is_some(),
        ];
        if present.iter().any(|value| *value) && present.iter().any(|value| !*value) {
            return verify_err!(
                self.loc(context),
                "allocation.binding adapter attributes must be all present or all absent"
            );
        }
        if let Some(driver) = driver
            && (driver.as_str().is_empty()
                || path.as_ref().is_none_or(|path| path.as_str().is_empty()))
        {
            return verify_err!(
                self.loc(context),
                "allocation.binding adapter driver and profile path must be non-empty"
            );
        }
        if digest.is_some_and(|digest| !is_sha256(digest.as_str())) {
            return verify_err!(
                self.loc(context),
                "allocation.binding adapter profile digest must be a lowercase SHA-256"
            );
        }
        Ok(())
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
