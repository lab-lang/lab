//! Built-in portable methods for the compiler's current biological vertical slice.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use lab_capability::{
    AbsoluteIri, CapabilityKind, ConstraintRelation, ControlMode, MethodId, OperationId,
    PropertyKind, QualificationLevel, UnitIri,
};
use lab_method::{
    CapabilityConstraintDefinition, CapabilityRequirementDefinition, IntentOperationId, LocalId,
    MethodDefinition, MethodInput, MethodOutput, MethodParameter, MethodRegistry, ParameterType,
    PortType, ProcedureParameterDefinition, ProcedureTaskDefinition, ProcedureValueExpression,
    ScalarType, ScalarValueExpression, TaskOutput, ValueReference,
};

const METHOD_NS: &str = "https://www.lab-compiler.org/ns/method#";
const PROCEDURE_NS: &str = "https://www.lab-compiler.org/ns/procedure#";
const STATE_NS: &str = "https://www.lab-compiler.org/ns/material-state#";
const CAPABILITY_NS: &str = "https://sbol.io/ns/capability#";
const QUDT_UNIT_NS: &str = "http://qudt.org/vocab/unit/";

/// Return the validated method catalog bundled with this compiler build.
pub fn standard_method_registry() -> &'static MethodRegistry {
    static REGISTRY: OnceLock<MethodRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        MethodRegistry::new(standard_method_definitions())
            .expect("bundled method definitions are validated by compiler tests")
    })
}

/// Return owned copies of the portable Method definitions bundled with this compiler build.
///
/// Frontends may extend this list before constructing their own validated registry. Facility and
/// adapter facts remain outside these definitions.
pub fn standard_method_definitions() -> Vec<MethodDefinition> {
    vec![
        artifact_realization_service(),
        automated_golden_gate(),
        material_provisioning(),
        chemical_transformation(),
        manual_recovery(),
        controlled_recovery(),
        serial_dilution(),
        antibiotic_selection(),
    ]
}

fn artifact_realization_service() -> MethodDefinition {
    let parameters = vec![
        parameter("artifact", ScalarType::Text),
        list_parameter("dependencies", ScalarType::Text),
    ];
    MethodDefinition {
        id: method("manual-artifact-realization"),
        refines: intent("std.bio.build.realize"),
        inputs: vec![input("design", PortType::Design)],
        parameters: parameters.clone(),
        tasks: vec![task(
            "realize",
            "RealizeArtifact",
            vec![input_ref("design")],
            vec![output("product", material("PlasmidProduct"))],
            select_parameters(&parameters, &["artifact", "dependencies"]),
            vec![requirement(
                "artifact-realization",
                "ArtifactRealization",
                [ControlMode::Manual],
                vec![],
            )],
        )],
        outputs: vec![method_output("product", "realize", "product")],
    }
}

fn automated_golden_gate() -> MethodDefinition {
    let parameters = realization_parameters();
    let setup_parameters = [
        "artifact",
        "backbone",
        "components",
        "dependencies",
        "restriction_enzyme",
        "assembly_replicates",
        "reaction_volume_ul",
        "part_volume_ul",
        "enzyme_volume_ul",
        "ligase_volume_ul",
        "buffer_volume_ul",
    ];
    let cycling_parameters = [
        "cycles",
        "digest_temperature_c",
        "digest_minutes",
        "ligate_temperature_c",
        "ligate_minutes",
    ];
    MethodDefinition {
        id: method("automated-golden-gate"),
        refines: intent("std.bio.build.realize"),
        inputs: vec![input("design", PortType::Design)],
        parameters: parameters.clone(),
        tasks: vec![
            task(
                "setup-reaction",
                "SetupGoldenGateReaction",
                vec![input_ref("design")],
                vec![output("reaction", material("AssemblyReaction"))],
                select_parameters(&parameters, &setup_parameters),
                vec![requirement(
                    "liquid-handling",
                    "LiquidHandling",
                    [ControlMode::ReviewedFile, ControlMode::Api],
                    vec![],
                )],
            ),
            task(
                "cycle-reaction",
                "ThermalCycleGoldenGateReaction",
                vec![task_ref("setup-reaction", "reaction")],
                vec![output("product", material("PlasmidProduct"))],
                select_parameters(&parameters, &cycling_parameters),
                vec![requirement(
                    "thermal-cycling",
                    "ThermalCycling",
                    [ControlMode::ReviewedFile, ControlMode::Api],
                    vec![],
                )],
            ),
        ],
        outputs: vec![method_output("product", "cycle-reaction", "product")],
    }
}

fn material_provisioning() -> MethodDefinition {
    let parameters = vec![parameter("item", ScalarType::Text)];
    MethodDefinition {
        id: method("manual-material-provisioning"),
        refines: intent("std.lab.plasmid.provision"),
        inputs: vec![],
        parameters: parameters.clone(),
        tasks: vec![task(
            "provision",
            "ProvisionMaterial",
            vec![],
            vec![output("material", material("CompetentCells"))],
            select_parameters(&parameters, &["item"]),
            vec![requirement(
                "material-provisioning",
                "MaterialProvisioning",
                [ControlMode::Manual],
                vec![],
            )],
        )],
        outputs: vec![method_output("material", "provision", "material")],
    }
}

fn chemical_transformation() -> MethodDefinition {
    let parameters = transformation_parameters();
    MethodDefinition {
        id: method("manual-chemical-transformation"),
        refines: intent("std.lab.plasmid.transform"),
        inputs: vec![
            input("design", PortType::Design),
            input("cells", material("CompetentCells")),
        ],
        parameters: parameters.clone(),
        tasks: vec![task(
            "transform",
            "ChemicallyTransformCells",
            vec![input_ref("design"), input_ref("cells")],
            vec![
                output("strain", material("StrainProduct")),
                output("culture", material("TransformedCulture")),
            ],
            parameters
                .iter()
                .map(|parameter| procedure_parameter(&parameter.name, parameter, None))
                .collect(),
            vec![requirement(
                "chemical-transformation",
                "ChemicalTransformation",
                [ControlMode::Manual],
                vec![],
            )],
        )],
        outputs: vec![
            method_output("strain", "transform", "strain"),
            method_output("culture", "transform", "culture"),
        ],
    }
}

fn manual_recovery() -> MethodDefinition {
    recovery_method("manual-recovery", [ControlMode::Manual])
}

fn controlled_recovery() -> MethodDefinition {
    recovery_method(
        "controlled-recovery",
        [
            ControlMode::ReviewedFile,
            ControlMode::VendorSession,
            ControlMode::Api,
            ControlMode::Sila2,
            ControlMode::OpcUa,
        ],
    )
}

fn recovery_method(
    name: &str,
    control_modes: impl IntoIterator<Item = ControlMode>,
) -> MethodDefinition {
    let duration = parameter("duration", ScalarType::Real);
    let value = ScalarValueExpression::IntentParameter {
        parameter: duration.name.clone(),
        unit: None,
    };
    MethodDefinition {
        id: method(name),
        refines: intent("std.lab.plasmid.recover"),
        inputs: vec![input("culture", material("TransformedCulture"))],
        parameters: vec![duration],
        tasks: vec![task(
            "recover",
            "RecoverCulture",
            vec![input_ref("culture")],
            vec![output("recovered", material("RecoveredCulture"))],
            vec![ProcedureParameterDefinition {
                id: local("duration"),
                property_kind: property("Duration"),
                value: ProcedureValueExpression::IntentParameter {
                    parameter: local("duration"),
                    unit: None,
                },
            }],
            vec![requirement(
                "incubation",
                "Incubation",
                control_modes,
                vec![CapabilityConstraintDefinition {
                    property_kind: property("Duration"),
                    relation: ConstraintRelation::Exact,
                    required: value,
                }],
            )],
        )],
        outputs: vec![method_output("recovered", "recover", "recovered")],
    }
}

fn serial_dilution() -> MethodDefinition {
    let parameters = vec![parameter("serial_dilutions", ScalarType::Integer)];
    MethodDefinition {
        id: method("serial-dilution"),
        refines: intent("std.lab.plasmid.dilute"),
        inputs: vec![input("culture", material("RecoveredCulture"))],
        parameters: parameters.clone(),
        tasks: vec![task(
            "dilute",
            "SeriallyDiluteCulture",
            vec![input_ref("culture")],
            vec![output("diluted", material("DilutedCulture"))],
            select_parameters(&parameters, &["serial_dilutions"]),
            vec![requirement(
                "liquid-handling",
                "LiquidHandling",
                [
                    ControlMode::Manual,
                    ControlMode::ReviewedFile,
                    ControlMode::Api,
                ],
                vec![],
            )],
        )],
        outputs: vec![method_output("diluted", "dilute", "diluted")],
    }
}

fn antibiotic_selection() -> MethodDefinition {
    let parameters = vec![
        parameter("selection", ScalarType::Text),
        parameter("replicates", ScalarType::Integer),
    ];
    MethodDefinition {
        id: method("manual-antibiotic-selection"),
        refines: intent("std.lab.plasmid.plate"),
        inputs: vec![input("culture", material("DilutedCulture"))],
        parameters: parameters.clone(),
        tasks: vec![task(
            "plate",
            "PlateCultureForSelection",
            vec![input_ref("culture")],
            vec![output("plate", material("Plate"))],
            parameters
                .iter()
                .map(|parameter| procedure_parameter(&parameter.name, parameter, None))
                .collect(),
            vec![requirement(
                "antibiotic-selection",
                "AntibioticSelection",
                [ControlMode::Manual],
                vec![],
            )],
        )],
        outputs: vec![method_output("plate", "plate", "plate")],
    }
}

fn realization_parameters() -> Vec<MethodParameter> {
    [
        parameter("artifact", ScalarType::Text),
        parameter("backbone", ScalarType::Text),
        list_parameter("components", ScalarType::Text),
        list_parameter("dependencies", ScalarType::Text),
        parameter("restriction_enzyme", ScalarType::Text),
    ]
    .into_iter()
    .chain(std::iter::once(parameter(
        "assembly_replicates",
        ScalarType::Integer,
    )))
    .chain(
        [
            "reaction_volume_ul",
            "part_volume_ul",
            "enzyme_volume_ul",
            "ligase_volume_ul",
            "buffer_volume_ul",
            "cycles",
            "digest_temperature_c",
            "digest_minutes",
            "ligate_temperature_c",
            "ligate_minutes",
        ]
        .map(|name| parameter(name, ScalarType::Integer)),
    )
    .collect()
}

fn transformation_parameters() -> Vec<MethodParameter> {
    [
        parameter("artifact", ScalarType::Text),
        parameter("chassis", ScalarType::Text),
        list_parameter("plasmids", ScalarType::Text),
        list_parameter("dependencies", ScalarType::Text),
        parameter("replicates", ScalarType::Integer),
    ]
    .into_iter()
    .chain(
        [
            "cell_volume_ul",
            "dna_volume_ul",
            "recovery_volume_ul",
            "cold_minutes",
            "heat_shock_temperature_c",
            "heat_shock_minutes",
            "recovery_temperature_c",
            "recovery_minutes",
            "medium_volume_ul",
            "culture_volume_ul",
            "colony_volume_ul",
        ]
        .map(|name| parameter(name, ScalarType::Integer)),
    )
    .collect()
}

fn select_parameters(
    parameters: &[MethodParameter],
    selected: &[&str],
) -> Vec<ProcedureParameterDefinition> {
    selected
        .iter()
        .map(|name| {
            let parameter = parameters
                .iter()
                .find(|parameter| parameter.name.as_str() == *name)
                .expect("built-in task parameters name declared method parameters");
            let unit = parameter_unit(name);
            procedure_parameter(&parameter.name, parameter, unit)
        })
        .collect()
}

fn parameter_unit(name: &str) -> Option<UnitIri> {
    if name.ends_with("_ul") {
        Some(unit("MicroL"))
    } else if name.ends_with("_temperature_c") {
        Some(unit("DEG_C"))
    } else if name.ends_with("_minutes") {
        Some(unit("MIN"))
    } else {
        None
    }
}

fn procedure_parameter(
    id: &LocalId,
    source: &MethodParameter,
    unit: Option<UnitIri>,
) -> ProcedureParameterDefinition {
    let unit = unit.or_else(|| parameter_unit(id.as_str()));
    ProcedureParameterDefinition {
        id: id.clone(),
        property_kind: property(&upper_camel(id.as_str())),
        value: ProcedureValueExpression::IntentParameter {
            parameter: source.name.clone(),
            unit,
        },
    }
}

fn task(
    id: &str,
    operation: &str,
    inputs: Vec<ValueReference>,
    outputs: Vec<TaskOutput>,
    parameters: Vec<ProcedureParameterDefinition>,
    requirements: Vec<CapabilityRequirementDefinition>,
) -> ProcedureTaskDefinition {
    ProcedureTaskDefinition {
        id: local(id),
        operation: OperationId::new(format!("{PROCEDURE_NS}{operation}")).unwrap(),
        inputs,
        outputs,
        parameters,
        materials: Vec::new(),
        requirements,
    }
}

fn requirement(
    id: &str,
    capability: &str,
    control_modes: impl IntoIterator<Item = ControlMode>,
    constraints: Vec<CapabilityConstraintDefinition>,
) -> CapabilityRequirementDefinition {
    CapabilityRequirementDefinition {
        id: local(id),
        capability_kind: CapabilityKind::new(format!("{CAPABILITY_NS}{capability}")).unwrap(),
        minimum_qualification: QualificationLevel::Plannable,
        accepted_control_modes: control_modes.into_iter().collect::<BTreeSet<_>>(),
        constraints,
    }
}

fn input(name: &str, port_type: PortType) -> MethodInput {
    MethodInput {
        name: local(name),
        port_type,
    }
}

fn parameter(name: &str, scalar_type: ScalarType) -> MethodParameter {
    MethodParameter {
        name: local(name),
        value_type: ParameterType::Scalar { scalar_type },
    }
}

fn list_parameter(name: &str, element_type: ScalarType) -> MethodParameter {
    MethodParameter {
        name: local(name),
        value_type: ParameterType::List { element_type },
    }
}

fn output(name: &str, port_type: PortType) -> TaskOutput {
    TaskOutput {
        name: local(name),
        port_type,
    }
}

fn input_ref(input: &str) -> ValueReference {
    ValueReference::Input {
        input: local(input),
    }
}

fn task_ref(task: &str, output: &str) -> ValueReference {
    ValueReference::TaskOutput {
        task: local(task),
        output: local(output),
    }
}

fn method_output(name: &str, task: &str, output: &str) -> MethodOutput {
    MethodOutput {
        name: local(name),
        source: task_ref(task, output),
    }
}

fn material(state: &str) -> PortType {
    PortType::Material {
        state: AbsoluteIri::new(format!("{STATE_NS}{state}")).unwrap(),
    }
}

fn method(name: &str) -> MethodId {
    MethodId::new(format!("{METHOD_NS}{name}")).unwrap()
}

fn intent(name: &str) -> IntentOperationId {
    IntentOperationId::new(name).unwrap()
}

fn local(name: &str) -> LocalId {
    LocalId::new(name).unwrap()
}

fn property(name: &str) -> PropertyKind {
    PropertyKind::new(format!("{CAPABILITY_NS}{name}")).unwrap()
}

fn unit(name: &str) -> UnitIri {
    UnitIri::new(format!("{QUDT_UNIT_NS}{name}")).unwrap()
}

fn upper_camel(value: &str) -> String {
    value
        .split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut characters = segment.chars();
            characters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(characters).collect()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_methods_validate_and_retain_real_alternatives() {
        let registry = standard_method_registry();
        let realization = registry.methods_for(&intent("std.bio.build.realize"));
        let recovery = registry.methods_for(&intent("std.lab.plasmid.recover"));

        assert_eq!(realization.len(), 2);
        assert_eq!(realization[0].id, method("automated-golden-gate"));
        assert_eq!(recovery.len(), 2);
        assert!(realization.iter().all(|method| method.validate().is_ok()));
    }

    #[test]
    fn built_in_registry_covers_every_current_workflow_intent() {
        for operation in [
            "std.bio.build.realize",
            "std.lab.plasmid.provision",
            "std.lab.plasmid.transform",
            "std.lab.plasmid.recover",
            "std.lab.plasmid.dilute",
            "std.lab.plasmid.plate",
        ] {
            assert!(
                !standard_method_registry()
                    .methods_for(&intent(operation))
                    .is_empty()
            );
        }
    }
}
