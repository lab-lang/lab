use std::ops::{Deref, DerefMut};

mod action_contract;
mod context;
mod declarations;
mod expr;
mod interface;
mod pattern;
mod workflow;

use context::SemanticContext;
use interface::build_interface;

use crate::ast::*;
use crate::checked::*;
use crate::semantic_error::SemanticError;
use crate::semantics::{ModuleId, SemanticEnvironment};
use crate::type_system::{Ty, to_checked_type};

pub(crate) fn check_module(
    module_id: ModuleId,
    environment: &SemanticEnvironment,
    module: &Module,
) -> Result<CheckedModule, SemanticError> {
    Checker::new(module_id, environment.clone()).check(module)
}

struct Checker {
    context: SemanticContext,
}

impl Deref for Checker {
    type Target = SemanticContext;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl DerefMut for Checker {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.context
    }
}

impl Checker {
    fn new(module_id: ModuleId, provided_modules: SemanticEnvironment) -> Self {
        Self {
            context: SemanticContext::new(module_id, provided_modules),
        }
    }

    fn check(mut self, module: &Module) -> Result<CheckedModule, SemanticError> {
        self.resolve_imports(module)?;
        self.collect_declarations(module)?;

        let mut declarations = Vec::new();
        for item in &module.items {
            match item {
                Item::Use(_) => {}
                Item::Circuit(declaration) => {
                    declarations.push(self.check_circuit(declaration)?);
                }
                Item::Artifact(declaration) => {
                    declarations.push(self.check_artifact(declaration)?);
                }
                Item::Data(declaration) => {
                    declarations.push(self.checked_data(declaration)?);
                }
                Item::Workflow(declaration) => {
                    declarations.push(self.check_workflow(declaration)?);
                }
                Item::Binding(binding) => {
                    let (checked, inferred) =
                        self.check_binding(binding, &mut self.values.clone())?;
                    self.values.insert(binding.names[0].value.clone(), inferred);
                    declarations.push(CheckedDeclaration::Binding(checked));
                }
            }
        }

        let interface = build_interface(&self.module_id, &declarations);
        Ok(CheckedModule {
            schema_version: PORTABLE_MODULE_SCHEMA_VERSION.to_owned(),
            module: self.module_id.clone(),
            interface,
            imports: self
                .imports
                .iter()
                .map(|module| ResolvedImport {
                    module: module.clone(),
                    provider: self
                        .import_providers
                        .get(module)
                        .cloned()
                        .unwrap_or_else(|| "builtin-standard-library".to_owned()),
                })
                .collect(),
            declarations,
        })
    }
}

pub(super) fn checked_field(name: &str, ty: &Ty) -> CheckedField {
    CheckedField {
        name: name.to_owned(),
        r#type: to_checked_type(ty),
    }
}

pub(super) fn path_text(path: &Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.value.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use crate::checker::*;
    use crate::semantics::DefinitionId;
    use crate::source::Span;
    use crate::standard_library::{PureFunctionSpec, StandardModule};
    use crate::{compile_module, compile_module_in_environment, compile_module_with_id};

    #[test]
    fn compiles_representative_design_module() {
        let module = compile_module(include_str!(
            "../../../docs/language/specimens/plasmid-design.lab"
        ))
        .unwrap();
        assert!(module.declarations.iter().any(|declaration| matches!(
            declaration,
            CheckedDeclaration::Artifact {
                artifact: ArtifactKind::Plasmid,
                ..
            }
        )));
    }

    #[test]
    fn emits_stable_module_interfaces_and_resolved_definition_ids() {
        let module = compile_module_with_id(
            ModuleId::new("reporter.design"),
            "use std.bio.parts\n\nreporter = sfGFP\n",
        )
        .unwrap();
        assert_eq!(module.module.as_str(), "reporter.design");
        assert_eq!(
            module.interface.exports["reporter"].definition,
            DefinitionId::exported("reporter.design", "reporter")
        );
        let CheckedDeclaration::Binding(binding) = &module.declarations[0] else {
            panic!("expected binding")
        };
        let CheckedExpression::Reference { definition, .. } = &binding.value.value else {
            panic!("expected resolved reference")
        };
        assert_eq!(
            definition,
            &DefinitionId::exported("std.bio.parts", "sfGFP")
        );
    }

    #[test]
    fn compiles_representative_reactive_workflow_module() {
        let module = compile_module(include_str!(
            "../../../docs/language/specimens/plasmid-build.lab"
        ))
        .unwrap();
        assert!(module.declarations.iter().any(|declaration| matches!(
            declaration,
            CheckedDeclaration::Workflow { name, .. } if name == "build_plasmid"
        )));
    }

    /// The Golden Gate example package, in an order that puts each module's
    /// imports in the environment before it compiles.
    const GOLDEN_GATE: [(&str, &str); 6] = [
        (
            "golden_gate.designs.inventory",
            include_str!("../../../examples/golden-gate/src/designs/inventory.lab"),
        ),
        (
            "golden_gate.designs.plasmids",
            include_str!("../../../examples/golden-gate/src/designs/plasmids.lab"),
        ),
        (
            "golden_gate.designs.strains",
            include_str!("../../../examples/golden-gate/src/designs/strains.lab"),
        ),
        (
            "golden_gate.workflows.assemble",
            include_str!("../../../examples/golden-gate/src/workflows/assemble.lab"),
        ),
        (
            "golden_gate.workflows.build_strains",
            include_str!("../../../examples/golden-gate/src/workflows/build_strains.lab"),
        ),
        (
            "golden_gate.programs.reporter_panel",
            include_str!("../../../examples/golden-gate/src/programs/reporter_panel.lab"),
        ),
    ];

    #[test]
    fn compiles_the_golden_gate_example_with_symbolic_inventory_names() {
        let mut environment = SemanticEnvironment::default();
        let mut modules = Vec::new();
        for (name, source) in GOLDEN_GATE {
            let module = compile_module_in_environment(ModuleId::new(name), source, &environment)
                .unwrap_or_else(|error| panic!("{name} must compile: {error}"));
            environment.insert(name, module.interface.clone());
            modules.push(module);
        }

        let declarations = modules
            .iter()
            .flat_map(|module| module.declarations.iter())
            .collect::<Vec<_>>();
        assert!(declarations.iter().any(|declaration| matches!(
            declaration,
            CheckedDeclaration::Artifact {
                artifact: ArtifactKind::Plasmid,
                ..
            }
        )));
        assert!(declarations.iter().any(|declaration| matches!(
            declaration,
            CheckedDeclaration::Artifact {
                artifact: ArtifactKind::Strain,
                ..
            }
        )));
        assert!(
            declarations
                .iter()
                .any(|declaration| matches!(declaration, CheckedDeclaration::Workflow { .. }))
        );

        // A component list names inventory identities imported from another
        // module, and stays a structured list of references rather than
        // collapsing into strings.
        let components = declarations
            .iter()
            .find_map(|declaration| {
                let CheckedDeclaration::Artifact {
                    name, properties, ..
                } = declaration
                else {
                    return None;
                };
                (name == "composite_plasmid_1").then(|| {
                    properties
                        .iter()
                        .find(|property| property.name == "components")
                        .unwrap()
                })
            })
            .unwrap();
        assert_eq!(components.value.r#type.display_name(), "List<Part>");
        let CheckedExpression::List { elements } = &components.value.value else {
            panic!("components must remain a structured checked list");
        };
        assert!(
            elements
                .iter()
                .all(|element| matches!(&element.value, CheckedExpression::Reference { .. }))
        );
    }

    /// A composite assembly can carry an already-assembled plasmid alongside
    /// bare parts, which the Golden Gate example has no occasion to do.
    #[test]
    fn a_component_list_admits_both_plasmids_and_parts() {
        let module = compile_module(
            r#"use std.bio.inventory

pSB1C3 = backbone("pSB1C3")
BsaI = restriction_enzyme("BsaI")
J23101 = part("J23101")
GFP = part("GFP")

plasmid promoter_carrier:
  sequence: dna("GCTAGCGGATCCATGACCATGATTACGCCAAGCTTGAATTC")
  backbone: pSB1C3
  components: [J23101]
  restriction_enzyme: BsaI
  require topology == circular
  accept sequence == design.sequence

plasmid reporter_region:
  sequence: dna("GATCCTCTAGAGTCGACCTGCAGGCATGCAAGCTTGGCACT")
  backbone: pSB1C3
  components: [promoter_carrier, GFP]
  restriction_enzyme: BsaI
  require topology == circular
  accept sequence == design.sequence
"#,
        )
        .unwrap();

        let components = module
            .declarations
            .iter()
            .find_map(|declaration| {
                let CheckedDeclaration::Artifact {
                    name, properties, ..
                } = declaration
                else {
                    return None;
                };
                (name == "reporter_region").then(|| {
                    properties
                        .iter()
                        .find(|property| property.name == "components")
                        .unwrap()
                })
            })
            .unwrap();
        assert_eq!(
            components.value.r#type.display_name(),
            "List<Plasmid | Part>"
        );
        let CheckedExpression::List { elements } = &components.value.value else {
            panic!("components must remain a structured checked list");
        };
        assert!(
            elements
                .iter()
                .all(|element| matches!(&element.value, CheckedExpression::Reference { .. }))
        );
    }

    #[test]
    fn checks_named_workflow_results_and_multi_result_calls() {
        let module = compile_module(
            r#"workflow preserve(
  product: Material<Plasmid>,
  plate: Material<Plate>,
) -> (
  product: Material<Plasmid>,
  plate: Material<Plate>,
):
  return product, plate

workflow delegate(
  product: Material<Plasmid>,
  plate: Material<Plate>,
) -> (
  product: Material<Plasmid>,
  plate: Material<Plate>,
):
  preserved_product, preserved_plate <- preserve product plate
  return preserved_product, preserved_plate
"#,
        )
        .unwrap();

        let CheckedDeclaration::Workflow { outputs, .. } = &module.declarations[0] else {
            panic!("expected workflow")
        };
        assert_eq!(outputs[0].name, "product");
        assert_eq!(outputs[1].name, "plate");
        let callable = module.interface.exports["preserve"]
            .callable
            .as_ref()
            .unwrap();
        assert_eq!(callable.outputs, *outputs);

        let CheckedDeclaration::Workflow { body, .. } = &module.declarations[1] else {
            panic!("expected workflow")
        };
        let CheckedStatement::Effect { action, .. } = &body[0] else {
            panic!("expected workflow call")
        };
        assert_eq!(action.operation, "workflow.preserve");
        assert_eq!(action.results[0].name, "product");
        assert_eq!(action.results[1].name, "plate");
    }

    #[test]
    fn rejects_named_workflow_return_arity_and_type_mismatches() {
        let duplicate = compile_module(
            r#"workflow invalid() -> (
  value: Integer,
  value: String,
):
  return 1, "one"
"#,
        )
        .unwrap_err();
        assert!(
            duplicate
                .to_string()
                .contains("duplicate workflow result 'value'"),
            "{duplicate}"
        );

        let arity = compile_module(
            r#"workflow invalid(product: Material<Plasmid>) -> (
  product: Material<Plasmid>,
  count: Integer,
):
  return product
"#,
        )
        .unwrap_err();
        assert!(
            arity
                .to_string()
                .contains("workflow returns 1 value(s), expected 2"),
            "{arity}"
        );

        let r#type = compile_module(
            r#"workflow invalid(product: Material<Plasmid>) -> (
  product: Material<Plasmid>,
  count: Integer,
):
  return product, "many"
"#,
        )
        .unwrap_err();
        assert!(
            r#type
                .to_string()
                .contains("workflow result 'count' has type String, expected Integer"),
            "{type}"
        );
    }

    #[test]
    fn imports_named_workflow_result_interfaces() {
        let provider = compile_module_with_id(
            ModuleId::new("tools"),
            r#"workflow pair(left: Integer, right: String) -> (
  left: Integer,
  right: String,
):
  return left, right
"#,
        )
        .unwrap();
        let environment = SemanticEnvironment::new([provider.interface]);
        let consumer = compile_module_in_environment(
            ModuleId::new("consumer"),
            r#"use tools

workflow forward(left: Integer, right: String) -> (
  left: Integer,
  right: String,
):
  forwarded_left, forwarded_right <- pair left right
  return forwarded_left, forwarded_right
"#,
            &environment,
        )
        .unwrap();

        let CheckedDeclaration::Workflow { body, .. } = &consumer.declarations[0] else {
            panic!("expected workflow")
        };
        let CheckedStatement::Effect { action, .. } = &body[0] else {
            panic!("expected imported workflow call")
        };
        assert_eq!(action.results[0].name, "left");
        assert_eq!(action.results[1].name, "right");
    }

    #[test]
    fn inventory_constructors_require_their_standard_module() {
        let error = compile_module("J23101 = part(\"J23101\")\n").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("requires 'use std.bio.inventory'")
        );
    }

    #[test]
    fn imported_standard_modules_reject_ambiguous_exports() {
        let mut checker = Checker::new(ModuleId::standalone(), SemanticEnvironment::default());
        checker
            .register_standard_module(
                StandardModule::new("std.first").with_values([("shared", Ty::String)]),
                Span::at(0),
            )
            .unwrap();
        let error = checker
            .register_standard_module(
                StandardModule::new("std.second").with_functions([PureFunctionSpec::new(
                    "shared",
                    "std.second.shared",
                    Vec::new(),
                    Ty::String,
                )]),
                Span::at(0),
            )
            .unwrap_err();

        let message = error.to_string();
        assert!(message.contains("std.first"));
        assert!(message.contains("std.second"));
        assert!(message.contains("shared"));
    }

    #[test]
    fn rejects_unknown_modules() {
        let error = compile_module("use mystery.catalog\n").unwrap_err();
        assert!(error.to_string().contains("cannot be resolved"));
    }

    const OPTIONAL_CLAUSE: &str = r#"
use std.bio.build
use std.bio.inventory

J23101 = part("J23101")
pSB1C3 = backbone("pSB1C3")
BsaI = restriction_enzyme("BsaI")

plasmid p_gfp:
  sequence: dna("ACGT")
  backbone: pSB1C3
  components: [J23101]
  restriction_enzyme: BsaI
  require topology == circular
  accept sequence == design.sequence

workflow assemble() -> Material<Plasmid>:
  product <- realize p_gfp
  return product
"#;

    #[test]
    fn an_omitted_optional_clause_binds_its_operand_to_the_empty_list() {
        let module = compile_module(OPTIONAL_CLAUSE).unwrap();
        let CheckedDeclaration::Workflow { body, .. } = module
            .declarations
            .iter()
            .find(|declaration| matches!(declaration, CheckedDeclaration::Workflow { .. }))
            .unwrap()
        else {
            unreachable!()
        };
        let CheckedStatement::Effect { action, .. } = &body[0] else {
            panic!("expected the realization effect");
        };
        let dependencies = action
            .arguments
            .iter()
            .find(|argument| argument.name == "dependencies")
            .expect("an omitted clause still supplies its operand");
        assert!(
            matches!(
                &dependencies.value.value,
                CheckedExpression::List { elements } if elements.is_empty()
            ),
            "{:?}",
            dependencies.value.value
        );
    }

    #[test]
    fn stating_an_optional_clause_is_equivalent_to_omitting_it() {
        let stated = OPTIONAL_CLAUSE.replace(
            "  product <- realize p_gfp\n",
            "  dependencies = []\n  product <- realize p_gfp from dependencies\n",
        );
        let omitted = compile_module(OPTIONAL_CLAUSE).unwrap();
        let stated = compile_module(&stated).unwrap();

        let arguments = |module: &CheckedModule| {
            let CheckedDeclaration::Workflow { body, .. } = module
                .declarations
                .iter()
                .find(|declaration| matches!(declaration, CheckedDeclaration::Workflow { .. }))
                .unwrap()
            else {
                unreachable!()
            };
            body.iter()
                .find_map(|statement| match statement {
                    CheckedStatement::Effect { action, .. } => Some(action.arguments.clone()),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(arguments(&omitted).len(), arguments(&stated).len());
    }

    #[test]
    fn a_partly_written_optional_clause_is_still_malformed() {
        let error = compile_module(&OPTIONAL_CLAUSE.replace(
            "  product <- realize p_gfp\n",
            "  product <- realize p_gfp from\n",
        ))
        .expect_err("naming the clause commits to its operand");
        assert!(error.to_string().contains("dependencies"), "{error}");
    }

    #[test]
    fn an_unknown_trailing_word_is_not_mistaken_for_an_omitted_clause() {
        let error = compile_module(&OPTIONAL_CLAUSE.replace(
            "  product <- realize p_gfp\n",
            "  product <- realize p_gfp onto bench\n",
        ))
        .expect_err("a phrase the contract does not describe stays an error");
        assert!(error.to_string().contains("malformed"), "{error}");
    }

    #[test]
    fn checks_durable_action_operand_types() {
        let error = compile_module(
            r#"use std.lab.plasmid_actions

workflow invalid(image: Image) -> Evidence:
  evidence <- quantify image
  return evidence
"#,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("expects Material<Plasmid>, found Image")
        );
    }

    #[test]
    fn lowers_action_capability_ownership_and_result_contract() {
        let module = compile_module(
            r#"use std.lab.plasmid_actions

workflow preserve(plasmid: Material<Plasmid>) -> Material<Plasmid>:
  plasmid <- store plasmid at -20 C
  return plasmid
"#,
        )
        .unwrap();

        let CheckedDeclaration::Workflow { body, .. } = &module.declarations[0] else {
            panic!("expected workflow")
        };
        let CheckedStatement::Effect { action, .. } = &body[0] else {
            panic!("expected effect")
        };
        assert_eq!(action.operation, "std.lab.plasmid_actions.store");
        assert_eq!(action.capability.as_deref(), Some("cold_storage"));
        assert_eq!(action.arguments[0].mode, OwnershipMode::Take);
        assert_eq!(action.results[0].name, "material");
        assert_eq!(action.results[0].r#type.display_name(), "Material<Plasmid>");
    }

    #[test]
    fn lowers_explicit_state_and_state_updates() {
        let module = compile_module(
            r#"workflow counter() -> Integer:
  state count: Integer = 0
  count = count + 1
  return count
"#,
        )
        .unwrap();

        let CheckedDeclaration::Workflow { state, body, .. } = &module.declarations[0] else {
            panic!("expected workflow")
        };
        assert_eq!(state.len(), 1);
        assert_eq!(state[0].name, "count");
        assert!(matches!(
            &body[0],
            CheckedStatement::StateUpdate { state, .. } if state == "count"
        ));
    }

    #[test]
    fn rejects_reassigning_an_ordinary_binding() {
        let error = compile_module(
            r#"workflow invalid() -> Integer:
  count = 0
  count = count + 1
  return count
"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("cannot reassign 'count'"));
        assert!(error.to_string().contains("with 'state'"));
    }

    #[test]
    fn rejects_state_after_executable_statements() {
        let error = compile_module(
            r#"workflow invalid() -> Integer:
  count = 0
  state remembered: Integer = count
  return remembered
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("state declarations must appear before workflow statements")
        );
    }
}
