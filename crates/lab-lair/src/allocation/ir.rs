//! Exact facility decisions attached to selected Procedure and Capability operations.

use std::cell::Ref;
use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::method::{IntentOperationId, LocalId};
use lab_capability::{
    AbsoluteIri, ConstraintRelation, ControlMode, ExactDecimal, ExactInteger, MethodId,
    ProcedureImplementationId, PropertyConstraint, PropertyKind, PropertyValue, QualificationLevel,
    ScalarValue, UnitIri,
};
use pliron::builtin::attributes::{StringAttr, VecAttr};
use pliron::builtin::op_interfaces::{NOpdsInterface, NResultsInterface};
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::pliron_op;
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::verify_err;

use crate::ir::attributes::string_vec;
use crate::planning::{
    SelectedAdapter, SelectedCapabilityParameter, SelectedMaterialBinding, SelectedMaterialSource,
    SelectedMethod, SelectedRequirementBinding,
};
use crate::procedure::ir::is_stable_local_id;

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
        bound_choice: StringAttr,
        interchangeable_alternatives: VecAttr
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
        result.set_attr_interchangeable_alternatives(
            context,
            string_vec(binding.interchangeable_alternatives.clone()),
        );
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

    #[allow(dead_code, reason = "consumed by the allocated-LAIR extractor")]
    pub(crate) fn interchangeable_alternatives(&self, context: &Context) -> Vec<String> {
        self.get_attr_interchangeable_alternatives(context)
            .expect("verified allocation.material carries interchangeable alternatives")
            .0
            .iter()
            .map(|value| {
                value
                    .downcast_ref::<StringAttr>()
                    .expect("verified interchangeable alternatives are strings")
                    .as_str()
                    .to_owned()
            })
            .collect()
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
        let Some(alternatives) = self.get_attr_interchangeable_alternatives(context) else {
            return verify_err!(
                self.loc(context),
                "allocation.material is missing interchangeable_alternatives"
            );
        };
        let mut seen_alternatives = BTreeSet::new();
        for alternative in &alternatives.0 {
            let Some(alternative) = alternative.downcast_ref::<StringAttr>() else {
                return verify_err!(
                    self.loc(context),
                    "allocation.material interchangeable alternatives must be strings"
                );
            };
            if AbsoluteIri::new(alternative.as_str()).is_err()
                || !seen_alternatives.insert(alternative.as_str().to_owned())
            {
                return verify_err!(
                    self.loc(context),
                    "allocation.material interchangeable alternatives must be absolute and unique IRIs"
                );
            }
        }
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
                    && material_lot
                        .as_ref()
                        .is_some_and(|selected| !seen_alternatives.contains(selected.as_str()))
                    && choice.is_none() =>
            {
                Ok(())
            }
            Some("choice_output")
                if component.is_none()
                    && material_lot.is_none()
                    && alternatives.0.is_empty()
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

/// Freezes one exact capability-constraint match against an offering parameter.
#[pliron_op(
    name = "allocation.parameter_match",
    format,
    attributes = (
        matched_requirement: StringAttr,
        matched_property_kind: StringAttr,
        matched_relation: StringAttr,
        matched_required_value_kind: StringAttr,
        matched_required_value: StringAttr,
        matched_required_unit: StringAttr,
        matched_offering_parameter: StringAttr,
        matched_observed_value_kind: StringAttr,
        matched_observed_value: StringAttr,
        matched_observed_unit: StringAttr
    ),
    interfaces = [NOpdsInterface<0>, NResultsInterface<0>]
)]
pub(crate) struct ParameterMatchOp;

impl ParameterMatchOp {
    pub(crate) fn new(
        context: &mut Context,
        requirement: &LocalId,
        parameter: &SelectedCapabilityParameter,
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
        result.set_attr_matched_requirement(context, StringAttr::new(requirement.to_string()));
        result.set_attr_matched_property_kind(
            context,
            StringAttr::new(parameter.property_kind.to_string()),
        );
        result.set_attr_matched_relation(context, StringAttr::new(parameter.relation.to_string()));
        let (required_kind, required_value) = encode_property_value(&parameter.required);
        result.set_attr_matched_required_value_kind(
            context,
            StringAttr::new(required_kind.to_owned()),
        );
        result.set_attr_matched_required_value(context, StringAttr::new(required_value));
        if let Some(unit) = &parameter.required.unit {
            result.set_attr_matched_required_unit(context, StringAttr::new(unit.to_string()));
        }
        result.set_attr_matched_offering_parameter(
            context,
            StringAttr::new(parameter.offering_parameter.clone()),
        );
        let (observed_kind, observed_value) = encode_property_value(&parameter.observed);
        result.set_attr_matched_observed_value_kind(
            context,
            StringAttr::new(observed_kind.to_owned()),
        );
        result.set_attr_matched_observed_value(context, StringAttr::new(observed_value));
        if let Some(unit) = &parameter.observed.unit {
            result.set_attr_matched_observed_unit(context, StringAttr::new(unit.to_string()));
        }
        result
    }

    pub(crate) fn requirement(&self, context: &Context) -> LocalId {
        LocalId::new(
            self.get_attr_matched_requirement(context)
                .expect("verified allocation.parameter_match carries a Requirement")
                .as_str(),
        )
        .expect("verified allocation.parameter_match Requirement is stable")
    }

    pub(crate) fn selected_parameter(&self, context: &Context) -> SelectedCapabilityParameter {
        self.decode(context)
            .expect("verified allocation.parameter_match carries exact typed values")
    }

    fn decode(
        &self,
        context: &Context,
    ) -> std::result::Result<SelectedCapabilityParameter, String> {
        let requirement = required_string_attr(
            self.get_attr_matched_requirement(context),
            "matched_requirement",
        )?;
        LocalId::new(&requirement)
            .map_err(|_| "matched_requirement must be a stable local ID".to_owned())?;
        let property_kind = PropertyKind::new(required_string_attr(
            self.get_attr_matched_property_kind(context),
            "matched_property_kind",
        )?)
        .map_err(|error| error.to_string())?;
        let relation = match required_string_attr(
            self.get_attr_matched_relation(context),
            "matched_relation",
        )?
        .as_str()
        {
            "exact" => ConstraintRelation::Exact,
            "at_least" => ConstraintRelation::AtLeast,
            "at_most" => ConstraintRelation::AtMost,
            other => return Err(format!("unknown constraint relation `{other}`")),
        };
        let required = decode_property_value(
            &required_string_attr(
                self.get_attr_matched_required_value_kind(context),
                "matched_required_value_kind",
            )?,
            &required_string_attr(
                self.get_attr_matched_required_value(context),
                "matched_required_value",
            )?,
            self.get_attr_matched_required_unit(context)
                .as_ref()
                .map(|unit| unit.as_str()),
        )?;
        let offering_parameter = required_string_attr(
            self.get_attr_matched_offering_parameter(context),
            "matched_offering_parameter",
        )?;
        AbsoluteIri::new(&offering_parameter).map_err(|error| error.to_string())?;
        let observed = decode_property_value(
            &required_string_attr(
                self.get_attr_matched_observed_value_kind(context),
                "matched_observed_value_kind",
            )?,
            &required_string_attr(
                self.get_attr_matched_observed_value(context),
                "matched_observed_value",
            )?,
            self.get_attr_matched_observed_unit(context)
                .as_ref()
                .map(|unit| unit.as_str()),
        )?;
        Ok(SelectedCapabilityParameter {
            property_kind,
            relation,
            required,
            offering_parameter,
            observed,
        })
    }
}

impl Verify for ParameterMatchOp {
    fn verify(&self, context: &Context) -> Result<()> {
        let parameter = match self.decode(context) {
            Ok(parameter) => parameter,
            Err(error) => {
                return verify_err!(
                    self.loc(context),
                    "invalid allocation.parameter_match: {error}"
                );
            }
        };
        let constraint = PropertyConstraint {
            property_kind: parameter.property_kind.clone(),
            relation: parameter.relation,
            required: parameter.required.clone(),
        };
        match constraint.is_satisfied_by(&parameter.observed) {
            Ok(true) => Ok(()),
            Ok(false) => verify_err!(
                self.loc(context),
                "allocation.parameter_match observed value does not satisfy its required constraint"
            ),
            Err(error) => verify_err!(
                self.loc(context),
                "allocation.parameter_match values are not comparable: {error}"
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
        adapter_driver: StringAttr,
        adapter_procedure_implementation: StringAttr,
        adapter_profile_path: StringAttr,
        adapter_profile_sha256: StringAttr,
        adapter_features: VecAttr,
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
        if let Some(adapter) = &binding.adapter {
            result.set_attr_adapter_driver(context, StringAttr::new(adapter.driver.clone()));
            if let Some(implementation) = &adapter.procedure_implementation {
                result.set_attr_adapter_procedure_implementation(
                    context,
                    StringAttr::new(implementation.to_string()),
                );
            }
            result.set_attr_adapter_profile_path(
                context,
                StringAttr::new(adapter.profile_path.to_string_lossy().into_owned()),
            );
            result.set_attr_adapter_profile_sha256(
                context,
                StringAttr::new(adapter.profile_sha256.clone()),
            );
            result.set_attr_adapter_features(
                context,
                string_vec(adapter.features.iter().cloned().collect()),
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

    #[allow(dead_code, reason = "consumed by the allocated-LAIR extractor")]
    pub(crate) fn selected_adapter(&self, context: &Context) -> Option<SelectedAdapter> {
        let driver = self.get_attr_adapter_driver(context)?;
        Some(SelectedAdapter {
            driver: driver.as_str().to_owned(),
            procedure_implementation: self.get_attr_adapter_procedure_implementation(context).map(
                |implementation| {
                    ProcedureImplementationId::new(implementation.as_str()).expect(
                        "verified allocation.binding Procedure implementation is an absolute IRI",
                    )
                },
            ),
            profile_path: PathBuf::from(
                self.get_attr_adapter_profile_path(context)
                    .expect("verified allocation.binding adapter carries a profile path")
                    .as_str(),
            ),
            profile_sha256: self
                .get_attr_adapter_profile_sha256(context)
                .expect("verified allocation.binding adapter carries a profile digest")
                .as_str()
                .to_owned(),
            features: string_set(
                &self
                    .get_attr_adapter_features(context)
                    .expect("verified allocation.binding adapter carries features"),
            ),
            accepted_run_formats: string_set(
                &self
                    .get_attr_adapter_accepted_run_formats(context)
                    .expect("verified allocation.binding adapter carries accepted run formats"),
            ),
            emitted_run_formats: string_set(
                &self
                    .get_attr_adapter_emitted_run_formats(context)
                    .expect("verified allocation.binding adapter carries emitted run formats"),
            ),
        })
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
        self.verify_adapter(context)
    }
}

impl BindingOp {
    fn verify_adapter(&self, context: &Context) -> Result<()> {
        let driver = self.get_attr_adapter_driver(context);
        let implementation = self.get_attr_adapter_procedure_implementation(context);
        let path = self.get_attr_adapter_profile_path(context);
        let digest = self.get_attr_adapter_profile_sha256(context);
        let features = self.get_attr_adapter_features(context);
        let accepted = self.get_attr_adapter_accepted_run_formats(context);
        let emitted = self.get_attr_adapter_emitted_run_formats(context);
        let present = [
            driver.is_some(),
            path.is_some(),
            digest.is_some(),
            features.is_some(),
            accepted.is_some(),
            emitted.is_some(),
        ];
        if present.iter().any(|value| *value) && present.iter().any(|value| !*value) {
            return verify_err!(
                self.loc(context),
                "allocation.binding adapter attributes must be all present or all absent"
            );
        }
        if implementation.is_some() && driver.is_none() {
            return verify_err!(
                self.loc(context),
                "allocation.binding cannot carry a Procedure implementation without an adapter"
            );
        }
        if implementation.is_some_and(|implementation| {
            ProcedureImplementationId::new(implementation.as_str()).is_err()
        }) {
            return verify_err!(
                self.loc(context),
                "allocation.binding adapter Procedure implementation must be an absolute IRI"
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
        for (name, values) in [
            ("features", features),
            ("accepted run formats", accepted),
            ("emitted run formats", emitted),
        ] {
            if values.is_some_and(|values| !is_non_empty_unique_string_set(&values)) {
                return verify_err!(
                    self.loc(context),
                    "allocation.binding adapter {name} must contain non-empty unique strings"
                );
            }
        }
        Ok(())
    }
}

#[allow(dead_code, reason = "consumed by the allocated-LAIR extractor")]
fn string_set(values: &VecAttr) -> BTreeSet<String> {
    values
        .0
        .iter()
        .map(|value| {
            value
                .downcast_ref::<StringAttr>()
                .expect("verified string sets contain strings")
                .as_str()
                .to_owned()
        })
        .collect()
}

fn is_non_empty_unique_string_set(values: &VecAttr) -> bool {
    let mut seen = BTreeSet::new();
    values.0.iter().all(|value| {
        value.downcast_ref::<StringAttr>().is_some_and(|value| {
            !value.as_str().is_empty() && seen.insert(value.as_str().to_owned())
        })
    })
}

fn required_string_attr(
    value: Option<Ref<'_, StringAttr>>,
    name: &str,
) -> std::result::Result<String, String> {
    value
        .map(|value| value.as_str().to_owned())
        .ok_or_else(|| format!("missing {name}"))
}

fn encode_property_value(value: &PropertyValue) -> (&'static str, String) {
    match &value.value {
        ScalarValue::Text(value) => ("text", value.clone()),
        ScalarValue::Integer(value) => ("integer", value.to_string()),
        ScalarValue::Real(value) => ("real", value.to_string()),
        ScalarValue::Boolean(value) => ("boolean", value.to_string()),
        ScalarValue::Iri(value) => ("iri", value.to_string()),
    }
}

fn decode_property_value(
    value_kind: &str,
    lexical: &str,
    unit: Option<&str>,
) -> std::result::Result<PropertyValue, String> {
    let value = match value_kind {
        "text" => ScalarValue::Text(lexical.to_owned()),
        "integer" => {
            ScalarValue::Integer(ExactInteger::parse(lexical).map_err(|error| error.to_string())?)
        }
        "real" => {
            ScalarValue::Real(ExactDecimal::parse(lexical).map_err(|error| error.to_string())?)
        }
        "boolean" => ScalarValue::Boolean(match lexical {
            "true" => true,
            "false" => false,
            _ => return Err("boolean value must be `true` or `false`".to_owned()),
        }),
        "iri" => ScalarValue::Iri(AbsoluteIri::new(lexical).map_err(|error| error.to_string())?),
        other => return Err(format!("unknown scalar value kind `{other}`")),
    };
    let unit = unit
        .map(|unit| UnitIri::new(unit).map_err(|error| error.to_string()))
        .transpose()?;
    PropertyValue::new(value, unit).map_err(|error| error.to_string())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use lab_capability::{CapabilityKind, ExactInteger};
    use pliron::builtin::op_interfaces::SingleBlockRegionInterface;
    use pliron::builtin::ops::ModuleOp;
    use pliron::identifier::Identifier;
    use pliron::operation::verify_operation;
    use pliron::printable::Printable;

    use crate::session::CompilerSession;

    use super::*;

    #[test]
    fn allocated_facts_verify_and_round_trip_without_loss() {
        let mut context = Context::new();
        let requirement = LocalId::new("task::requirement::temperature").unwrap();
        let unit = UnitIri::new("http://qudt.org/vocab/unit/DEG_C").unwrap();
        let parameter = SelectedCapabilityParameter {
            property_kind: PropertyKind::new("https://example.org/property/temperature").unwrap(),
            relation: ConstraintRelation::AtLeast,
            required: PropertyValue::new(
                ScalarValue::Integer(ExactInteger::parse("37").unwrap()),
                Some(unit.clone()),
            )
            .unwrap(),
            offering_parameter: "https://example.org/offering/temperature".to_owned(),
            observed: PropertyValue::new(
                ScalarValue::Real(ExactDecimal::parse("37.5").unwrap()),
                Some(unit),
            )
            .unwrap(),
        };
        let parameter_match = ParameterMatchOp::new(&mut context, &requirement, &parameter);
        assert_eq!(parameter_match.requirement(&context), requirement);
        assert_eq!(parameter_match.selected_parameter(&context), parameter);

        let material = SelectedMaterialBinding {
            input: LocalId::new("task::material::sample").unwrap(),
            symbol: "sample".to_owned(),
            source: SelectedMaterialSource::MaterialLot {
                component: "https://example.org/component/sample".to_owned(),
                material_lot: "https://example.org/lot/selected".to_owned(),
            },
            interchangeable_alternatives: vec![
                "https://example.org/lot/alternative-a".to_owned(),
                "https://example.org/lot/alternative-b".to_owned(),
            ],
        };
        let procedure_node = LocalId::new("task").unwrap();
        let material_binding = MaterialBindingOp::new(&mut context, &procedure_node, &material);
        assert_eq!(
            material_binding.interchangeable_alternatives(&context),
            material.interchangeable_alternatives
        );

        let adapter = SelectedAdapter {
            driver: "example.driver".to_owned(),
            procedure_implementation: Some(
                ProcedureImplementationId::new("https://example.org/procedure/implementation")
                    .unwrap(),
            ),
            profile_path: PathBuf::from("adapters/example.toml"),
            profile_sha256: "a".repeat(64),
            features: ["temperature-control".to_owned()].into_iter().collect(),
            accepted_run_formats: ["application/json".to_owned()].into_iter().collect(),
            emitted_run_formats: ["text/plain".to_owned()].into_iter().collect(),
        };
        let binding = SelectedRequirementBinding {
            requirement: requirement.clone(),
            capability_kind: CapabilityKind::new("https://example.org/capability/temperature")
                .unwrap(),
            minimum_qualification: QualificationLevel::Executable,
            accepted_control_modes: [ControlMode::Api].into_iter().collect(),
            offering: "https://example.org/offering/temperature-control".to_owned(),
            asset: "https://example.org/asset/incubator".to_owned(),
            observed_qualification: QualificationLevel::Qualified.to_string(),
            control_mode: ControlMode::Api.to_string(),
            parameters: vec![parameter],
            adapter: Some(adapter.clone()),
            rejected_candidates: Vec::new(),
        };
        let allocation_binding = BindingOp::new(&mut context, &procedure_node, &binding);
        assert_eq!(allocation_binding.selected_adapter(&context), Some(adapter));

        let module = ModuleOp::new(
            &mut context,
            Identifier::try_from("allocated_facts").unwrap(),
        );
        module.append_operation(&mut context, material_binding.get_operation(), 0);
        module.append_operation(&mut context, allocation_binding.get_operation(), 0);
        module.append_operation(&mut context, parameter_match.get_operation(), 0);
        verify_operation(module.get_operation(), &context).unwrap();

        let ir = module.get_operation().disp(&context).to_string();
        assert!(ir.contains("allocation.parameter_match"), "{ir}");
        assert!(ir.contains("matched_observed_value"), "{ir}");
        assert!(ir.contains("adapter_procedure_implementation"), "{ir}");
        assert!(ir.contains("adapter_features"), "{ir}");
        assert!(ir.contains("interchangeable_alternatives"), "{ir}");
        assert!(!ir.contains("matched_offering_parameters"), "{ir}");

        let mut reparsed = CompilerSession::default();
        reparsed.parse_ir(&ir).unwrap();
        reparsed.verify().unwrap();
        let reparsed_ir = reparsed.ir().unwrap();
        for fact in [
            "allocation.parameter_match",
            "matched_observed_value",
            "adapter_procedure_implementation",
            "adapter_features",
            "interchangeable_alternatives",
        ] {
            assert!(reparsed_ir.contains(fact), "{reparsed_ir}");
        }
    }

    #[test]
    fn parameter_matches_reject_unsatisfied_observations() {
        let mut context = Context::new();
        let parameter = SelectedCapabilityParameter {
            property_kind: PropertyKind::new("https://example.org/property/count").unwrap(),
            relation: ConstraintRelation::AtLeast,
            required: PropertyValue::unitless(ScalarValue::Integer(
                ExactInteger::parse("5").unwrap(),
            )),
            offering_parameter: "https://example.org/offering/count".to_owned(),
            observed: PropertyValue::unitless(ScalarValue::Integer(
                ExactInteger::parse("4").unwrap(),
            )),
        };
        let operation = ParameterMatchOp::new(
            &mut context,
            &LocalId::new("task::requirement::count").unwrap(),
            &parameter,
        );

        assert!(operation.verify(&context).is_err());
    }
}
