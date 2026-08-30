//! Built-in portable methods for the compiler's current biological vertical slice.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use lab_capability::{
    AbsoluteIri, CapabilityKind, ConstraintRelation, ControlMode, ExactInteger, MethodId,
    OperationId, PropertyKind, PropertyValue, QualificationLevel, ScalarValue, UnitIri,
};
use lab_method::{
    CapabilityConstraintDefinition, CapabilityRequirementDefinition, IntentOperationId, LocalId,
    MaterialInputDefinition, MaterialSourceExpression, MethodDefinition, MethodInput, MethodOutput,
    MethodParameter, MethodRegistry, ParameterType, PortType, ProcedureParameterDefinition,
    ProcedureTaskDefinition, ProcedureValue, ProcedureValueExpression, ScalarType,
    ScalarValueExpression, TaskOutput, ValueReference,
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
        temperature_staged_golden_gate(),
        material_provisioning(),
        automated_chemical_transformation(),
        manual_chemical_transformation(),
        manual_recovery(),
        controlled_recovery(),
        automated_recovery(),
        serial_dilution(),
        automated_antibiotic_selection(),
        manual_antibiotic_selection(),
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
            vec![material_parameter("dependencies", "dependencies")],
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
    golden_gate_method(
        "automated-golden-gate",
        GoldenGateSetupMethod::Basic {
            mix_cycles: 3,
            mix_volume_ul: 15,
        },
    )
}

fn temperature_staged_golden_gate() -> MethodDefinition {
    golden_gate_method(
        "temperature-staged-golden-gate",
        GoldenGateSetupMethod::TemperatureStaged {
            source_mix_cycles: 3,
            source_temperature_c: 4,
            bubble_clear_divisor_ul: 10,
            bubble_clear_max_volume_ul: 20,
            bubble_clear_dispense_offset_mm: 8,
        },
    )
}

enum GoldenGateSetupMethod {
    Basic {
        mix_cycles: u32,
        mix_volume_ul: u32,
    },
    TemperatureStaged {
        source_mix_cycles: u32,
        source_temperature_c: u32,
        bubble_clear_divisor_ul: u32,
        bubble_clear_max_volume_ul: u32,
        bubble_clear_dispense_offset_mm: u32,
    },
}

fn golden_gate_method(method_id: &str, setup: GoldenGateSetupMethod) -> MethodDefinition {
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
        "artifact",
        "assembly_replicates",
        "reaction_volume_ul",
        "cycles",
        "digest_temperature_c",
        "digest_minutes",
        "ligate_temperature_c",
        "ligate_minutes",
        "lid_temperature_c",
        "final_digest_temperature_c",
        "final_digest_minutes",
        "heat_inactivation_temperature_c",
        "heat_inactivation_minutes",
        "hold_temperature_c",
    ];
    let setup_task_parameters = match setup {
        GoldenGateSetupMethod::Basic {
            mix_cycles,
            mix_volume_ul,
        } => with_text_literal(
            with_integer_literals(
                select_parameters(&parameters, &setup_parameters),
                [("mix_cycles", mix_cycles), ("mix_volume_ul", mix_volume_ul)],
            ),
            "setup_strategy",
            "basic_v1",
        ),
        GoldenGateSetupMethod::TemperatureStaged {
            source_mix_cycles,
            source_temperature_c,
            bubble_clear_divisor_ul,
            bubble_clear_max_volume_ul,
            bubble_clear_dispense_offset_mm,
        } => with_text_literal(
            with_integer_literals(
                select_parameters(&parameters, &setup_parameters),
                [
                    ("source_mix_cycles", source_mix_cycles),
                    ("source_temperature_c", source_temperature_c),
                    ("bubble_clear_divisor_ul", bubble_clear_divisor_ul),
                    ("bubble_clear_max_volume_ul", bubble_clear_max_volume_ul),
                    (
                        "bubble_clear_dispense_offset_mm",
                        bubble_clear_dispense_offset_mm,
                    ),
                ],
            ),
            "setup_strategy",
            "temperature_staged_v1",
        ),
    };
    let cycling_task_parameters = select_parameters(&parameters, &cycling_parameters);
    MethodDefinition {
        id: method(method_id),
        refines: intent("std.bio.build.realize"),
        inputs: vec![input("design", PortType::Design)],
        parameters: parameters.clone(),
        tasks: vec![
            task(
                "setup-reaction",
                "SetupGoldenGateReaction",
                vec![input_ref("design")],
                vec![output("reaction", material("AssemblyReaction"))],
                setup_task_parameters,
                vec![
                    material_parameter("backbone", "backbone"),
                    material_parameter("components", "components"),
                    material_parameter("dependencies", "dependencies"),
                    material_parameter("restriction-enzyme", "restriction_enzyme"),
                    material_literal("ligase", "T4_DNA_ligase"),
                    material_literal("buffer", "T4_DNA_ligase_buffer"),
                    material_literal("water", "nuclease_free_water"),
                ],
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
                cycling_task_parameters,
                vec![],
                vec![requirement(
                    "thermal-cycling",
                    "ThermalCycling",
                    [
                        ControlMode::ReviewedFile,
                        ControlMode::Api,
                        ControlMode::Sila2,
                    ],
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
            vec![material_parameter("item", "item")],
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

fn automated_chemical_transformation() -> MethodDefinition {
    let parameters = transformation_parameters();
    MethodDefinition {
        id: method("automated-chemical-transformation"),
        refines: intent("std.lab.plasmid.transform"),
        inputs: vec![
            input("design", PortType::Design),
            input("cells", material("CompetentCells")),
        ],
        parameters: parameters.clone(),
        tasks: vec![
            task(
                "prepare-transformation",
                "PrepareChemicalTransformation",
                vec![input_ref("design"), input_ref("cells")],
                vec![output("mixture", material("TransformationMixture"))],
                with_integer_literals(
                    select_parameters(
                        &parameters,
                        &[
                            "artifact",
                            "chassis",
                            "plasmids",
                            "dependencies",
                            "replicates",
                            "dna_count",
                            "cell_volume_ul",
                            "dna_volume_ul",
                        ],
                    ),
                    [
                        ("cell_mix_cycles", 3),
                        ("cell_mix_volume_ul", 50),
                        ("dna_mix_cycles", 3),
                        ("bubble_clear_cycles", 2),
                        ("bubble_clear_volume_ul", 20),
                        ("bubble_clear_dispense_offset_mm", 8),
                    ],
                ),
                vec![material_parameter("dependencies", "dependencies")],
                vec![requirement(
                    "liquid-handling",
                    "LiquidHandling",
                    [ControlMode::ReviewedFile, ControlMode::Api],
                    vec![],
                )],
            ),
            task(
                "heat-shock",
                "HeatShockTransformation",
                vec![task_ref("prepare-transformation", "mixture")],
                vec![
                    output("strain", material("StrainProduct")),
                    output("culture", material("TransformedCulture")),
                ],
                with_integer_literals(
                    select_parameters(
                        &parameters,
                        &[
                            "artifact",
                            "replicates",
                            "dna_count",
                            "cell_volume_ul",
                            "dna_volume_ul",
                            "cold_minutes",
                            "heat_shock_temperature_c",
                            "heat_shock_minutes",
                        ],
                    ),
                    [
                        ("cold_temperature_c", 4),
                        ("post_shock_minutes", 2),
                        ("hold_temperature_c", 4),
                    ],
                ),
                vec![],
                vec![requirement(
                    "thermal-cycling",
                    "ThermalCycling",
                    [
                        ControlMode::ReviewedFile,
                        ControlMode::Api,
                        ControlMode::Sila2,
                    ],
                    vec![],
                )],
            ),
        ],
        outputs: vec![
            method_output("strain", "heat-shock", "strain"),
            method_output("culture", "heat-shock", "culture"),
        ],
    }
}

fn manual_chemical_transformation() -> MethodDefinition {
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
            vec![material_parameter("dependencies", "dependencies")],
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

fn automated_recovery() -> MethodDefinition {
    let parameters = recovery_parameters();
    MethodDefinition {
        id: method("automated-recovery"),
        refines: intent("std.lab.plasmid.recover"),
        inputs: vec![input("culture", material("TransformedCulture"))],
        parameters: parameters.clone(),
        tasks: vec![
            task(
                "add-medium",
                "AddRecoveryMedium",
                vec![input_ref("culture")],
                vec![output("mixture", material("RecoveryMixture"))],
                with_integer_literals(
                    select_parameters(
                        &parameters,
                        &[
                            "subject",
                            "replicates",
                            "initial_volume_ul",
                            "recovery_volume_ul",
                        ],
                    ),
                    [("air_gap_ul", 10)],
                ),
                vec![material_literal("medium", "recovery_medium")],
                vec![requirement(
                    "liquid-handling",
                    "LiquidHandling",
                    [ControlMode::ReviewedFile, ControlMode::Api],
                    vec![],
                )],
            ),
            task(
                "incubate",
                "IncubateRecoveryCulture",
                vec![task_ref("add-medium", "mixture")],
                vec![output("recovered", material("RecoveredCulture"))],
                with_integer_literals(
                    select_parameters(
                        &parameters,
                        &[
                            "subject",
                            "duration",
                            "replicates",
                            "initial_volume_ul",
                            "recovery_volume_ul",
                            "recovery_temperature_c",
                        ],
                    ),
                    [("hold_temperature_c", 4)],
                ),
                vec![],
                vec![requirement(
                    "thermal-incubation",
                    "ThermalCycling",
                    [
                        ControlMode::ReviewedFile,
                        ControlMode::Api,
                        ControlMode::Sila2,
                    ],
                    vec![],
                )],
            ),
        ],
        outputs: vec![method_output("recovered", "incubate", "recovered")],
    }
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
            vec![material_literal("medium", "recovery_medium")],
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

fn recovery_parameters() -> Vec<MethodParameter> {
    vec![
        parameter("subject", ScalarType::Text),
        parameter("duration", ScalarType::Real),
        parameter("replicates", ScalarType::Integer),
        parameter("initial_volume_ul", ScalarType::Integer),
        parameter("recovery_volume_ul", ScalarType::Integer),
        parameter("recovery_temperature_c", ScalarType::Integer),
    ]
}

fn serial_dilution() -> MethodDefinition {
    let parameters = vec![
        parameter("subject", ScalarType::Text),
        parameter("replicates", ScalarType::Integer),
        parameter("initial_volume_ul", ScalarType::Integer),
        parameter("serial_dilutions", ScalarType::Integer),
        parameter("medium_volume_ul", ScalarType::Integer),
        parameter("culture_volume_ul", ScalarType::Integer),
    ];
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
            with_integer_literals(
                select_parameters(
                    &parameters,
                    &[
                        "subject",
                        "replicates",
                        "initial_volume_ul",
                        "serial_dilutions",
                        "medium_volume_ul",
                        "culture_volume_ul",
                    ],
                ),
                [("mix_cycles", 5), ("mix_volume_ul", 19)],
            ),
            vec![material_literal("medium", "recovery_medium")],
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

fn automated_antibiotic_selection() -> MethodDefinition {
    let parameters = vec![
        parameter("subject", ScalarType::Text),
        parameter("selection", ScalarType::Text),
        parameter("replicates", ScalarType::Integer),
        parameter("culture_replicates", ScalarType::Integer),
        parameter("serial_dilutions", ScalarType::Integer),
        parameter("medium_volume_ul", ScalarType::Integer),
        parameter("culture_volume_ul", ScalarType::Integer),
        parameter("colony_volume_ul", ScalarType::Integer),
    ];
    MethodDefinition {
        id: method("automated-antibiotic-selection"),
        refines: intent("std.lab.plasmid.plate"),
        inputs: vec![input("culture", material("DilutedCulture"))],
        parameters: parameters.clone(),
        tasks: vec![task(
            "plate",
            "PlateDilutedCulture",
            vec![input_ref("culture")],
            vec![output("plate", material("Plate"))],
            parameters
                .iter()
                .map(|parameter| procedure_parameter(&parameter.name, parameter, None))
                .collect(),
            vec![material_parameter("selection", "selection")],
            vec![requirement(
                "liquid-handling",
                "LiquidHandling",
                [ControlMode::ReviewedFile, ControlMode::Api],
                vec![],
            )],
        )],
        outputs: vec![method_output("plate", "plate", "plate")],
    }
}

fn manual_antibiotic_selection() -> MethodDefinition {
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
            vec![material_parameter("selection", "selection")],
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
            "lid_temperature_c",
            "final_digest_temperature_c",
            "final_digest_minutes",
            "heat_inactivation_temperature_c",
            "heat_inactivation_minutes",
            "hold_temperature_c",
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
        parameter("dna_count", ScalarType::Integer),
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
    } else if name.ends_with("_mm") {
        Some(unit("MilliM"))
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

fn with_integer_literals<const N: usize>(
    mut parameters: Vec<ProcedureParameterDefinition>,
    values: [(&str, u32); N],
) -> Vec<ProcedureParameterDefinition> {
    for (name, value) in values {
        let id = local(name);
        parameters.retain(|parameter| parameter.id != id);
        parameters.push(literal_integer_parameter(name, value));
    }
    parameters
}

fn literal_integer_parameter(name: &str, value: u32) -> ProcedureParameterDefinition {
    let value = PropertyValue::new(
        ScalarValue::Integer(ExactInteger::parse(value.to_string()).unwrap()),
        parameter_unit(name),
    )
    .expect("built-in integer Procedure literals use valid units");
    ProcedureParameterDefinition {
        id: local(name),
        property_kind: property(&upper_camel(name)),
        value: ProcedureValueExpression::Literal {
            value: ProcedureValue::Scalar { value },
        },
    }
}

fn with_text_literal(
    mut parameters: Vec<ProcedureParameterDefinition>,
    name: &str,
    value: &str,
) -> Vec<ProcedureParameterDefinition> {
    let id = local(name);
    parameters.retain(|parameter| parameter.id != id);
    let value = PropertyValue::new(ScalarValue::Text(value.to_owned()), None)
        .expect("built-in text Procedure literals are unitless");
    parameters.push(ProcedureParameterDefinition {
        id,
        property_kind: property(&upper_camel(name)),
        value: ProcedureValueExpression::Literal {
            value: ProcedureValue::Scalar { value },
        },
    });
    parameters
}

fn task(
    id: &str,
    operation: &str,
    inputs: Vec<ValueReference>,
    outputs: Vec<TaskOutput>,
    parameters: Vec<ProcedureParameterDefinition>,
    materials: Vec<MaterialInputDefinition>,
    requirements: Vec<CapabilityRequirementDefinition>,
) -> ProcedureTaskDefinition {
    ProcedureTaskDefinition {
        id: local(id),
        operation: OperationId::new(format!("{PROCEDURE_NS}{operation}")).unwrap(),
        inputs,
        outputs,
        parameters,
        materials,
        requirements,
    }
}

fn material_parameter(id: &str, parameter: &str) -> MaterialInputDefinition {
    MaterialInputDefinition {
        id: local(id),
        source: MaterialSourceExpression::IntentParameter {
            parameter: local(parameter),
        },
    }
}

fn material_literal(id: &str, symbol: &str) -> MaterialInputDefinition {
    MaterialInputDefinition {
        id: local(id),
        source: MaterialSourceExpression::Literal {
            symbol: symbol.to_owned(),
        },
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
        let transformation = registry.methods_for(&intent("std.lab.plasmid.transform"));
        let plating = registry.methods_for(&intent("std.lab.plasmid.plate"));

        assert_eq!(realization.len(), 3);
        assert_eq!(realization[0].id, method("automated-golden-gate"));
        assert!(
            realization
                .iter()
                .any(|candidate| candidate.id == method("temperature-staged-golden-gate"))
        );
        assert_eq!(recovery.len(), 3);
        assert_eq!(transformation.len(), 2);
        assert_eq!(plating.len(), 2);
        assert!(
            realization
                .iter()
                .chain(recovery)
                .chain(transformation)
                .chain(plating)
                .all(|method| method.validate().is_ok())
        );
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
