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

pub(crate) fn check_module_with_library(
    module_id: ModuleId,
    environment: &SemanticEnvironment,
    module: &Module,
    library: crate::standard_library::StandardLibrary,
) -> Result<CheckedModule, SemanticError> {
    Checker {
        context: SemanticContext::with_library(module_id, environment.clone(), library),
    }
    .check(module)
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
                Item::Role(declaration) => {
                    declarations.push(CheckedDeclaration::Role {
                        doc: declaration.doc.clone(),
                        name: declaration.name.value.clone(),
                    });
                }
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

        let interface = build_interface(&self.module_id, module.doc.as_deref(), &declarations);
        Ok(CheckedModule {
            schema_version: PORTABLE_MODULE_SCHEMA_VERSION.to_owned(),
            module: self.module_id.clone(),
            doc: module.doc.clone(),
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
    use crate::ModuleError;
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
    fn documentation_travels_in_the_checked_module_and_its_interface() {
        let module = compile_module(
            "/*! Synthetic reporter designs. */\n\n/** A synthetic reporter plasmid. */\nplasmid reporter:\n  sequence: dna(\"ACGT\")\n",
        )
        .expect("the module checks");

        assert_eq!(module.doc.as_deref(), Some("Synthetic reporter designs."));
        assert_eq!(
            module.interface.documentation, "Synthetic reporter designs.",
            "an importer sees what the module documents"
        );

        let CheckedDeclaration::Artifact { doc, .. } = &module.declarations[0] else {
            panic!("the declaration is an artifact");
        };
        assert_eq!(doc.as_deref(), Some("A synthetic reporter plasmid."));
        assert_eq!(
            module.interface.exports["reporter"].documentation, "A synthetic reporter plasmid.",
            "an importer sees what the declaration documents"
        );
    }

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

    /// A circuit is only reusable if its type parameters survive being
    /// exported: a library of circuits parameterized over sensors is the point
    /// of having parameters at all.
    #[test]
    fn imports_generic_circuit_signatures() {
        let provider = compile_module_with_id(
            ModuleId::new("sensors"),
            r#"circuit regulated_expression(
  promoter: Promoter<Trigger: Signal>,
  coding: CDS<Product: Protein>,
) -> Circuit<Trigger, Product>:
  layout:
    promoter
    coding
"#,
        )
        .unwrap();

        // The parameters and their bounds are part of the exported contract.
        // Without the bounds an importer would accept an argument the defining
        // module rejects.
        let exported = &provider.interface.exports["regulated_expression"].parameters;
        assert_eq!(exported.names, ["Trigger", "Product"]);
        assert_eq!(exported.bounds["Trigger"].display_name(), "Signal");
        assert_eq!(exported.bounds["Product"].display_name(), "Protein");

        let environment = SemanticEnvironment::new([provider.interface]);
        let consumer = compile_module_in_environment(
            ModuleId::new("consumer"),
            "use std.bio.parts\nuse sensors\n\ntet_reporter = regulated_expression(pTet, sfGFP)\n",
            &environment,
        )
        .expect("an imported generic circuit is callable");

        let CheckedDeclaration::Binding(binding) = &consumer.declarations[0] else {
            panic!("expected a binding")
        };
        assert_eq!(
            binding.targets[0].r#type.display_name(),
            "Circuit<Tetracycline, GreenFluorescentProtein>",
            "the caller's argument types are substituted into the result"
        );
    }

    /// A circuit over an `Inducer` role the test module declares itself.
    const SENSOR_CIRCUIT: &str = r#"circuit regulated_expression(
  promoter: Promoter<Trigger: Inducer>,
  coding: CDS<Product: Inducer>,
) -> Circuit<Trigger, Product>:
  layout:
    promoter
    coding
"#;

    /// The point of roles: a scientist classifies their own type and every
    /// existing generic circuit accepts it.
    #[test]
    fn a_type_declared_in_source_can_play_a_role_and_satisfy_a_bound() {
        let module = compile_module(&format!(
            "role Inducer\n\nrecord Arabinose is Inducer\n\n{SENSOR_CIRCUIT}"
        ))
        .expect("a source-declared role bounds a circuit parameter");

        let CheckedDeclaration::Data { name, roles, .. } = &module.declarations[1] else {
            panic!("expected the record: {:?}", module.declarations[1]);
        };
        assert_eq!(name, "Arabinose");
        assert_eq!(roles, &["Inducer"]);

        let CheckedDeclaration::Circuit {
            parameters, bounds, ..
        } = &module.declarations[2]
        else {
            panic!("expected the circuit: {:?}", module.declarations[2]);
        };
        assert_eq!(
            parameters,
            &["Trigger", "Product"],
            "parameters are harvested in the order the signature introduces them"
        );
        assert_eq!(bounds["Trigger"].display_name(), "Inducer");
    }

    #[test]
    fn a_role_is_not_a_type_and_says_what_to_write_instead() {
        let error = compile_module(
            "role Inducer\n\nrecord Arabinose is Inducer\n\nrecord Run:\n  used: Inducer\n",
        )
        .expect_err("a value cannot be a category");

        let ModuleError::Semantic(error) = error else {
            panic!("expected a semantic error: {error:?}");
        };
        assert_eq!(error.message, "'Inducer' is a role, not a type");
        assert!(
            error.help.iter().any(|help| help.contains("<T: Inducer>")),
            "naming the parameter is one way forward: {:?}",
            error.help
        );
        assert!(
            error
                .help
                .iter()
                .any(|help| help.contains("or name a type that plays Inducer: Arabinose")),
            "the alternatives are named: {:?}",
            error.help
        );
    }

    /// A bound on a data declaration's own parameter constrains every argument
    /// the type is given, which is what makes it a bound rather than a note.
    #[test]
    fn a_bound_on_a_declared_type_is_enforced_where_the_type_is_used() {
        const VOCABULARY: &str = r#"role Inducer

record Arabinose is Inducer

record Glucose

record Sensor<T: Inducer>:
  target: T
"#;

        compile_module(&format!(
            "{VOCABULARY}\nrecord Panel:\n  sensor: Sensor<Arabinose>\n"
        ))
        .expect("a member of the role satisfies the bound");

        let error = compile_module(&format!(
            "{VOCABULARY}\nrecord Panel:\n  sensor: Sensor<Glucose>\n"
        ))
        .unwrap_err();
        let ModuleError::Semantic(error) = error else {
            panic!("expected a semantic error: {error:?}");
        };
        assert_eq!(error.message, "'Glucose' does not play the role Inducer");
        assert!(
            error
                .help
                .iter()
                .any(|help| help.contains("types that play Inducer: Arabinose")),
            "the alternatives are named: {:?}",
            error.help
        );
        assert!(
            error
                .help
                .iter()
                .any(|help| help.contains("declare 'Glucose is Inducer'")),
            "the fix is named: {:?}",
            error.help
        );
    }

    /// Tetracycline plays Signal, so a circuit bounded by a role it does not
    /// play must refuse it — the bound is nominal, not structural.
    #[test]
    fn a_circuit_bound_failure_names_the_types_that_would_satisfy_it() {
        let error = compile_module(
            r#"use std.bio.parts

role Inducer

record Rhamnose is Inducer

circuit sense(promoter: Promoter<S: Inducer>) -> Promoter<S>:
  layout:
    promoter

bad = sense(pTet)
"#,
        )
        .unwrap_err();

        let ModuleError::Semantic(error) = error else {
            panic!("expected a semantic error: {error:?}");
        };
        assert_eq!(
            error.message,
            "'Tetracycline' does not play the role Inducer"
        );
        assert!(
            error
                .help
                .iter()
                .any(|help| help.contains("circuit 'sense' requires its 'S'")),
            "{:?}",
            error.help
        );
        assert!(
            error
                .help
                .iter()
                .any(|help| help.contains("types that play Inducer: Rhamnose")),
            "{:?}",
            error.help
        );
    }

    #[test]
    fn rejects_membership_in_something_that_is_not_a_role() {
        let error = compile_module("record Arabinose is Plasmid\n").unwrap_err();
        let ModuleError::Semantic(error) = error else {
            panic!("expected a semantic error: {error:?}");
        };
        assert_eq!(error.message, "'Plasmid' is not a role");
        assert!(
            error
                .help
                .iter()
                .any(|help| help.contains("declare 'role Plasmid'")),
            "{:?}",
            error.help
        );

        let unknown = compile_module("record Arabinose is Nowhere\n").unwrap_err();
        assert!(unknown.to_string().contains("'Nowhere' is not a role"));
    }

    #[test]
    fn a_role_and_a_type_cannot_share_a_name() {
        let error = compile_module("role Signal\n").unwrap_err();
        assert!(
            error.to_string().contains("'Signal' is already a role"),
            "the prelude already declares Signal as a role: {error}"
        );

        let shadowed = compile_module("record Plasmid\n").unwrap_err();
        assert!(
            shadowed.to_string().contains("'Plasmid' is already a type"),
            "{shadowed}"
        );

        let ModuleError::Semantic(collision) =
            compile_module("role Inducer\n\nrecord Inducer\n").unwrap_err()
        else {
            panic!("expected a semantic error")
        };
        assert_eq!(collision.message, "duplicate declaration 'Inducer'");
        assert_eq!(
            collision.related[0].message, "'Inducer' is already declared here",
            "the role it collides with is pointed at"
        );
    }

    /// Roles are only useful if a package can classify a type against a role
    /// another package declared.
    #[test]
    fn roles_and_membership_cross_module_boundaries() {
        let vocabulary =
            compile_module_with_id(ModuleId::new("vocabulary"), "role Inducer\n").unwrap();
        let mut environment = SemanticEnvironment::new([vocabulary.interface]);

        let catalog = compile_module_in_environment(
            ModuleId::new("catalog"),
            "use vocabulary\n\nrecord Arabinose is Inducer\n",
            &environment,
        )
        .expect("a package may classify its type against an imported role");
        environment.insert("catalog", catalog.interface);

        let circuits = compile_module_in_environment(
            ModuleId::new("circuits"),
            &format!("use vocabulary\n\n{SENSOR_CIRCUIT}"),
            &environment,
        )
        .expect("a circuit may bound a parameter by an imported role");
        environment.insert("circuits", circuits.interface);

        // The membership declared in `catalog` has to reach `program`, which
        // imports it only indirectly through the types it names.
        let program = compile_module_in_environment(
            ModuleId::new("program"),
            "use catalog\nuse circuits\nuse vocabulary\n",
            &environment,
        )
        .expect("importing all three resolves");
        assert!(program.declarations.is_empty());
    }

    /// A bound is only a bound if it survives export. An importer that cannot
    /// see a type's parameters cannot check the arguments it is given.
    #[test]
    fn imports_generic_type_parameters_and_checks_their_bounds() {
        let vocabulary = compile_module_with_id(
            ModuleId::new("vocabulary"),
            "role Inducer\n\nrecord Arabinose is Inducer\n\nrecord Glucose\n\nrecord Sensor<T: Inducer>:\n  target: T\n",
        )
        .unwrap();

        let exported = &vocabulary.interface.exports["Sensor"];
        assert_eq!(exported.parameters.names, ["T"]);
        assert_eq!(exported.parameters.bounds["T"].display_name(), "Inducer");

        let environment = SemanticEnvironment::new([vocabulary.interface]);
        compile_module_in_environment(
            ModuleId::new("good"),
            "use vocabulary\n\nrecord Panel:\n  sensor: Sensor<Arabinose>\n",
            &environment,
        )
        .expect("a member of the imported role satisfies the imported bound");

        let error = compile_module_in_environment(
            ModuleId::new("bad"),
            "use vocabulary\n\nrecord Panel:\n  sensor: Sensor<Glucose>\n",
            &environment,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("'Glucose' does not play the role Inducer"),
            "{error}"
        );

        let arity = compile_module_in_environment(
            ModuleId::new("arity"),
            "use vocabulary\n\nrecord Panel:\n  sensor: Sensor\n",
            &environment,
        )
        .unwrap_err();
        assert!(
            arity
                .to_string()
                .contains("type 'Sensor' expects 1 argument(s), found 0"),
            "{arity}"
        );
    }

    /// A parameter is introduced where it is first needed, so reading order and
    /// binding order are the same. These are the rules that keep them so.
    mod inline_type_parameters {
        use super::*;

        fn check(signature: &str) -> Result<CheckedModule, ModuleError> {
            compile_module(&format!(
                "role Inducer\n\nrecord Arabinose is Inducer\n\ncircuit sense{signature}:\n  layout:\n    promoter\n"
            ))
        }

        fn semantic(signature: &str) -> SemanticError {
            match check(signature).unwrap_err() {
                ModuleError::Semantic(error) => error,
                other => panic!("expected a semantic error: {other:?}"),
            }
        }

        #[test]
        fn one_parameter_reaches_the_result_type() {
            check("(promoter: Promoter<S: Inducer>) -> Promoter<S>")
                .expect("the result may name a parameter a parameter introduced");
        }

        #[test]
        fn the_same_name_in_two_parameters_is_the_same_type() {
            let module =
                check("(promoter: Promoter<S: Inducer>, backup: Promoter<S>) -> Promoter<S>")
                    .expect("a later mention refers to the one already introduced");
            let CheckedDeclaration::Circuit { parameters, .. } = &module.declarations[2] else {
                panic!("expected the circuit")
            };
            assert_eq!(parameters, &["S"], "the name is introduced once");
        }

        #[test]
        fn a_result_type_cannot_name_something_never_introduced() {
            let error = semantic("(promoter: Promoter<S: Inducer>) -> Promoter<T>");
            assert_eq!(error.message, "unknown type 'T'");
            assert!(
                error
                    .help
                    .iter()
                    .any(|help| help == "this signature introduces 'S'"),
                "the names actually in scope are listed: {:?}",
                error.help
            );
        }

        #[test]
        fn a_name_cannot_be_introduced_twice() {
            let error = semantic(
                "(promoter: Promoter<S: Inducer>, backup: Promoter<S: Inducer>) -> Promoter<S>",
            );
            assert_eq!(error.message, "'S' is already introduced");
            assert_eq!(error.related[0].message, "'S' is introduced here");
            assert!(
                error
                    .help
                    .iter()
                    .any(|help| help.contains("write 'S' alone to mean the same one")),
                "{:?}",
                error.help
            );
        }

        #[test]
        fn a_name_cannot_be_used_before_it_is_introduced() {
            let error =
                semantic("(backup: Promoter<S>, promoter: Promoter<S: Inducer>) -> Promoter<S>");
            assert_eq!(error.message, "'S' is used before it is introduced");
            assert_eq!(error.related[0].message, "'S' is introduced here");
            assert!(
                error
                    .help
                    .iter()
                    .any(|help| help.contains("move ': Inducer' to the first place 'S' appears")),
                "{:?}",
                error.help
            );
        }

        #[test]
        fn a_bound_must_name_a_role() {
            let error = semantic("(promoter: Promoter<S: Arabinose>) -> Promoter<S>");
            assert_eq!(error.message, "'Arabinose' is not a role");
        }

        /// Outside a signature there is no caller to choose the type, so there
        /// is nothing for a binding to mean.
        #[test]
        fn a_parameter_cannot_be_introduced_outside_a_signature() {
            let error = match compile_module(
                "role Inducer\n\nrecord Holder:\n  slot: Promoter<S: Inducer>\n",
            )
            .unwrap_err()
            {
                ModuleError::Semantic(error) => error,
                other => panic!("expected a semantic error: {other:?}"),
            };
            assert_eq!(error.message, "'S' cannot be introduced here");
            assert!(
                error
                    .help
                    .iter()
                    .any(|help| help.contains("introduced by a circuit or workflow parameter")),
                "{:?}",
                error.help
            );
        }
    }

    /// A type parameter that reaches into orchestration is the point of the
    /// whole feature: the same name indexes the design and the reagent, so the
    /// compiler enforces that they match.
    mod generic_workflows {
        use super::*;

        /// `characterize` links its two operands: the circuit's signal and the
        /// inducer poured onto it must be the same one. Both are returned so
        /// the affine checker is satisfied without a disposal contract.
        const PANEL: &str = r#"role Inducer

record Arabinose is Inducer
record Doxycycline is Inducer

workflow characterize(
  design: Circuit<S: Inducer, S>,
  inducer: Material<S>,
) -> (
  design: Circuit<S, S>,
  inducer: Material<S>,
):
  return design, inducer
"#;

        const MAIN: &str = r#"
workflow main(
  dox: Circuit<Doxycycline, Doxycycline>,
  dox_stock: Material<Doxycycline>,
) -> (
  design: Circuit<Doxycycline, Doxycycline>,
  inducer: Material<Doxycycline>,
):
"#;

        fn program(body: &str) -> Result<CheckedModule, ModuleError> {
            compile_module(&format!("{PANEL}{MAIN}{body}"))
        }

        #[test]
        fn a_matching_reagent_is_accepted() {
            program("  design, inducer <- characterize dox dox_stock\n  return design, inducer\n")
                .expect("the inducer matches the signal the circuit responds to");
        }

        #[test]
        fn a_mismatched_reagent_names_both_operands() {
            let source = format!(
                "{PANEL}\nworkflow main(\n  dox: Circuit<Doxycycline, Doxycycline>,\n  ara_stock: Material<Arabinose>,\n) -> (\n  design: Circuit<Doxycycline, Doxycycline>,\n  inducer: Material<Arabinose>,\n):\n  design, inducer <- characterize dox ara_stock\n  return design, inducer\n"
            );
            let error = match compile_module(&source).unwrap_err() {
                ModuleError::Semantic(error) => error,
                other => panic!("expected a semantic error: {other:?}"),
            };

            assert_eq!(
                error.message,
                "'S' cannot be both Doxycycline and Arabinose"
            );
            assert_eq!(
                error.related[0].message, "this fixes S = Doxycycline",
                "the operand that decided the parameter is named"
            );
            assert_eq!(
                error.related[1].message, "this requires S = Arabinose",
                "and so is the one that contradicts it"
            );
            assert_ne!(
                error.related[0].span, error.related[1].span,
                "each operand is underlined where it actually appears"
            );
        }

        #[test]
        fn the_result_type_is_substituted_at_the_call_site() {
            let module = program(
                "  design, inducer <- characterize dox dox_stock\n  return design, inducer\n",
            )
            .unwrap();

            let CheckedDeclaration::Workflow { body, .. } = module
                .declarations
                .iter()
                .find(|declaration| {
                    matches!(declaration, CheckedDeclaration::Workflow { name, .. } if name == "main")
                })
                .unwrap()
            else {
                unreachable!()
            };
            let CheckedStatement::Effect { results, .. } = &body[0] else {
                panic!("expected the workflow call: {body:?}")
            };
            assert_eq!(
                results[0].r#type.display_name(),
                "Circuit<Doxycycline, Doxycycline>",
                "the parameter is substituted rather than left abstract"
            );
            assert_eq!(results[1].r#type.display_name(), "Material<Doxycycline>");
        }

        #[test]
        fn a_bound_is_enforced_on_a_workflow_operand() {
            let error = compile_module(&format!(
                "{PANEL}\nrecord Glucose\n\nworkflow main(\n  bad: Circuit<Glucose, Glucose>,\n  stock: Material<Glucose>,\n) -> (\n  design: Circuit<Glucose, Glucose>,\n  inducer: Material<Glucose>,\n):\n  design, inducer <- characterize bad stock\n  return design, inducer\n"
            ))
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("'Glucose' does not play the role Inducer"),
                "{error}"
            );
        }

        /// A bare type parameter is not a material, so a generic workflow does
        /// not accidentally acquire affine operands it must account for.
        #[test]
        fn a_type_parameter_is_not_itself_a_material() {
            compile_module(&format!(
                "{PANEL}\nworkflow observe(design: Circuit<S: Inducer, S>) -> Circuit<S, S>:\n  return design\n"
            ))
            .expect("a parameter carries no ownership of its own");
        }

        /// A list is a type argument like any other, so a workflow over a
        /// collection is as generic as one over a single value.
        #[test]
        fn a_parameter_nested_in_a_list_is_inferred_from_its_elements() {
            let module = compile_module(&format!(
                "{PANEL}\nworkflow keep(stocks: List<Material<S: Inducer>>) -> List<Material<S>>:\n  return stocks\n\nworkflow main(held: List<Material<Doxycycline>>) -> List<Material<Doxycycline>>:\n  kept <- keep held\n  return kept\n"
            ))
            .expect("the element type determines the parameter");

            let CheckedDeclaration::Workflow { body, .. } = module
                .declarations
                .iter()
                .find(|declaration| {
                    matches!(declaration, CheckedDeclaration::Workflow { name, .. } if name == "main")
                })
                .unwrap()
            else {
                unreachable!()
            };
            let CheckedStatement::Effect { results, .. } = &body[0] else {
                panic!("expected the workflow call")
            };
            assert_eq!(
                results[0].r#type.display_name(),
                "List<Material<Doxycycline>>"
            );
        }

        /// An empty list says nothing about what it would have held.
        #[test]
        fn an_empty_list_leaves_a_nested_parameter_unsolved() {
            let error = compile_module(&format!(
                "{PANEL}\nworkflow keep(stocks: List<Material<S: Inducer>>) -> List<Material<S>>:\n  return stocks\n\nworkflow main() -> None:\n  nothing = []\n  kept <- keep nothing\n  return None\n"
            ))
            .unwrap_err();
            assert!(error.to_string().contains("could not infer 'S'"), "{error}");
        }

        #[test]
        fn workflow_generics_cross_a_module_boundary() {
            let provider = compile_module_with_id(ModuleId::new("assays"), PANEL).unwrap();
            let exported = &provider.interface.exports["characterize"].parameters;
            assert_eq!(exported.names, ["S"]);
            assert_eq!(exported.bounds["S"].display_name(), "Inducer");

            let environment = SemanticEnvironment::new([provider.interface]);
            let error = compile_module_in_environment(
                ModuleId::new("program"),
                "use assays\n\nworkflow main(\n  dox: Circuit<Doxycycline, Doxycycline>,\n  ara_stock: Material<Arabinose>,\n) -> (\n  design: Circuit<Doxycycline, Doxycycline>,\n  inducer: Material<Arabinose>,\n):\n  design, inducer <- characterize dox ara_stock\n  return design, inducer\n",
                &environment,
            )
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("'S' cannot be both Doxycycline and Arabinose"),
                "an imported generic workflow still links its operands: {error}"
            );
        }
    }

    /// `any Signal` is some signal you will never learn the name of; `S: Signal`
    /// is some signal you named so you can point at it again.
    mod forgotten_type_arguments {
        use super::*;

        const PANEL: &str = r#"role Inducer
role Readout

record Arabinose is Inducer
record Doxycycline is Inducer
record Glucose

record Fluorescence is Readout
record Luminescence is Readout

workflow characterize(
  design: Circuit<S: Inducer, Fluorescence>,
  inducer: Material<S>,
) -> (
  design: Circuit<S, Fluorescence>,
  inducer: Material<S>,
):
  return design, inducer

workflow read_out(
  design: Circuit<any Inducer, Fluorescence>,
) -> Circuit<any Inducer, Fluorescence>:
  return design
"#;

        fn check(body: &str) -> Result<CheckedModule, ModuleError> {
            compile_module(&format!("{PANEL}{body}"))
        }

        fn semantic(body: &str) -> SemanticError {
            match check(body).unwrap_err() {
                ModuleError::Semantic(error) => error,
                other => panic!("expected a semantic error: {other:?}"),
            }
        }

        /// The panel: different triggers, one readout, so the results compare.
        #[test]
        fn concrete_arguments_pack_into_a_role_they_play() {
            check(
                "\nworkflow main(\n  ara: Circuit<Arabinose, Fluorescence>,\n  dox: Circuit<Doxycycline, Fluorescence>,\n) -> Circuit<any Inducer, Fluorescence>:\n  panel: List<Circuit<any Inducer, Fluorescence>> = [ara, dox]\n  checked <- read_out ara\n  return checked\n",
            )
            .expect("a list of differently-triggered circuits is a panel");
        }

        /// The asymmetry is the information: the trigger varies, the readout is
        /// pinned so the numbers mean something next to each other.
        #[test]
        fn the_pinned_argument_is_still_checked() {
            let error = semantic(
                "\nworkflow main(\n  ara: Circuit<Arabinose, Fluorescence>,\n  lux: Circuit<Doxycycline, Luminescence>,\n) -> Circuit<any Inducer, Fluorescence>:\n  panel: List<Circuit<any Inducer, Fluorescence>> = [ara, lux]\n  checked <- read_out ara\n  return checked\n",
            );
            assert!(
                error.message.contains("Luminescence"),
                "the readout may not vary even though the trigger may: {}",
                error.message
            );
            assert!(
                error
                    .help
                    .iter()
                    .any(|help| help == "'Luminescence' does not fit 'Fluorescence'"),
                "the failing position is named rather than left to be diffed: {:?}",
                error.help
            );
        }

        #[test]
        fn a_type_that_does_not_play_the_role_cannot_be_forgotten_into_it() {
            let error = semantic(
                "\nworkflow main(bad: Circuit<Glucose, Fluorescence>) -> Circuit<Glucose, Fluorescence>:\n  panel: List<Circuit<any Inducer, Fluorescence>> = [bad]\n  return bad\n",
            );
            assert!(
                error.message.contains("Glucose"),
                "Glucose plays no role, so it fits no existential: {}",
                error.message
            );
        }

        /// The load-bearing rule. Without it `S` binds to `any Inducer`,
        /// `Material<S>` accepts any inducer at all, and the wrong-reagent
        /// error stops firing.
        #[test]
        fn a_forgotten_argument_cannot_be_recovered_by_naming_it() {
            let error = semantic(
                "\nworkflow main(\n  panel_member: Circuit<any Inducer, Fluorescence>,\n  ara_stock: Material<Arabinose>,\n) -> (\n  design: Circuit<any Inducer, Fluorescence>,\n  inducer: Material<Arabinose>,\n):\n  design, inducer <- characterize panel_member ara_stock\n  return design, inducer\n",
            );
            assert_eq!(
                error.message,
                "'S' cannot be inferred from a forgotten type"
            );
            assert!(
                error
                    .help
                    .iter()
                    .any(|help| help.contains("deliberately not recorded")),
                "{:?}",
                error.help
            );
        }

        #[test]
        fn forgetting_does_not_run_backwards() {
            let error = semantic(
                "\nworkflow main(hidden: Circuit<any Inducer, Fluorescence>) -> Circuit<any Inducer, Fluorescence>:\n  named: Circuit<Arabinose, Fluorescence> = hidden\n  return hidden\n",
            );
            assert!(
                error.message.contains("any Inducer"),
                "a forgotten argument does not become a concrete one again: {}",
                error.message
            );
        }

        #[test]
        fn any_is_not_a_type_on_its_own() {
            let error =
                check("\nworkflow main(inducer: any Inducer) -> Evidence:\n  return inducer\n")
                    .unwrap_err();
            assert!(
                error.to_string().contains("'any' is not a type on its own"),
                "{error}"
            );
            assert!(
                error.to_string().contains("Material<any Signal>"),
                "the message shows what carrying one looks like: {error}"
            );
        }

        #[test]
        fn a_forgotten_material_stays_affine() {
            let module = check(
                "\nworkflow main(stock: Material<Arabinose>) -> Material<any Inducer>:\n  return stock\n",
            )
            .expect("packing a material does not launder its ownership");
            let CheckedDeclaration::Workflow { outputs, .. } = module
                .declarations
                .iter()
                .find(|declaration| {
                    matches!(declaration, CheckedDeclaration::Workflow { name, .. } if name == "main")
                })
                .unwrap()
            else {
                unreachable!()
            };
            assert_eq!(outputs[0].r#type.display_name(), "Material<any Inducer>");
        }
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
