use std::ops::{Deref, DerefMut};

mod action_contract;
mod context;
mod declarations;
mod expr;
mod interface;
mod ontology;
mod pattern;
mod workflow;

use context::SemanticContext;
use interface::build_interface;

use crate::ast::*;
use crate::checked::*;
use crate::semantic_error::SemanticError;
use crate::semantics::{ModuleId, SemanticEnvironment};
use crate::source::Span;
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
                        term: self.role_terms.get(&declaration.name.value).cloned(),
                    });
                }
                Item::Facet(declaration) => {
                    let signature = self
                        .facets
                        .get(&declaration.name.value)
                        .expect("the facet was collected");
                    declarations.push(CheckedDeclaration::Facet {
                        doc: declaration.doc.clone(),
                        name: declaration.name.value.clone(),
                        subject: to_checked_type(&signature.subject),
                        states: signature
                            .states
                            .iter()
                            .map(|state| CheckedFacetState {
                                doc: state.doc.clone(),
                                name: state.name.clone(),
                                fields: state
                                    .fields
                                    .iter()
                                    .map(|(name, field)| CheckedSchemaField {
                                        name: name.clone(),
                                        r#type: to_checked_type(&field.ty),
                                        optional: field.optional,
                                    })
                                    .collect(),
                            })
                            .collect(),
                        transitions: signature
                            .transitions
                            .iter()
                            .map(|(from, to)| CheckedFacetTransition {
                                from: from.clone(),
                                to: to.clone(),
                            })
                            .collect(),
                    });
                }
                Item::Action(declaration) => {
                    let contract = self
                        .action_contracts
                        .get(&declaration.name.value)
                        .expect("the action was collected");
                    declarations.push(CheckedDeclaration::Action {
                        doc: declaration.doc.clone(),
                        name: declaration.name.value.clone(),
                        operation: contract.operation.clone(),
                        phrase: contract.phrase.clone(),
                        operands: contract.operands.clone(),
                        results: contract.results.clone(),
                        capability: contract.capability.clone(),
                    });
                }
                Item::ArtifactKind(declaration) => {
                    let signature = self
                        .artifact_kinds
                        .get(&declaration.name.value)
                        .expect("the kind was collected");
                    declarations.push(CheckedDeclaration::ArtifactKind {
                        doc: declaration.doc.clone(),
                        name: declaration.name.value.clone(),
                        produces: to_checked_type(&signature.produces),
                        roles: declaration.roles.iter().map(path_text).collect(),
                        fields: signature
                            .fields
                            .iter()
                            .map(|(name, field)| CheckedSchemaField {
                                name: name.clone(),
                                r#type: to_checked_type(&field.ty),
                                optional: field.optional,
                            })
                            .collect(),
                        declares: signature.declares.clone(),
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

        self.check_evidence_supports_claims(module, &declarations)?;

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

impl Checker {
    /// Refuse a program that offers less independent evidence than the design
    /// it is judging asks for.
    ///
    /// Three measurements of one colony are one biological replicate however
    /// many times they are repeated, so counting them as three claims more than
    /// the experiment supports. This fires only where every lineage is known:
    /// a sample that arrived from a caller, or a member of a family picked at
    /// runtime, leaves the question open and nothing is said.
    fn check_evidence_supports_claims(
        &self,
        module: &Module,
        declarations: &[CheckedDeclaration],
    ) -> Result<(), SemanticError> {
        let required = declarations
            .iter()
            .filter_map(|declaration| match declaration {
                CheckedDeclaration::Artifact {
                    name, acceptance, ..
                } => {
                    let most = acceptance
                        .iter()
                        .filter_map(|claim| claim.replicates)
                        .max()?;
                    Some((name.as_str(), most))
                }
                _ => None,
            })
            .collect::<std::collections::HashMap<_, _>>();
        if required.is_empty() {
            return Ok(());
        }

        let table = crate::provenance::lineage_table(&self.standard_library);
        for declaration in declarations {
            let CheckedDeclaration::Workflow { name, body, .. } = declaration else {
                continue;
            };
            let lineage = crate::provenance::analyze(body, &table);
            let mut designs = std::collections::HashMap::new();
            let mut found = None;
            collect_acceptance_calls(body, &mut designs, &required, &lineage, &mut found);
            let Some((design, asked, offered)) = found else {
                continue;
            };
            let span = acceptance_call_span(module, name).unwrap_or_else(|| Span::at(0));
            let claim = if asked == 1 {
                "1 biological replicate".to_owned()
            } else {
                format!("{asked} biological replicates")
            };
            return Err(SemanticError::new(
                span,
                format!(
                    "'{design}' is accepted on {claim}, but this evidence spans {offered}"
                ),
            )
            .help(
                "measuring one sample repeatedly gives technical replicates, which measure handling rather than biology",
            )
            .help(
                "independent colonies, or independent transformations, give biological replicates",
            ));
        }
        Ok(())
    }
}

/// Walk a checked body for a judgement whose evidence falls short, following
/// bindings so a design named once and used later is still recognized.
fn collect_acceptance_calls(
    body: &[CheckedStatement],
    designs: &mut std::collections::HashMap<String, String>,
    required: &std::collections::HashMap<&str, u64>,
    lineage: &crate::provenance::LineageMap,
    found: &mut Option<(String, u64, usize)>,
) {
    for statement in body {
        match statement {
            CheckedStatement::Binding(binding) => {
                if let CheckedExpression::Reference { path, .. } = &binding.value.value
                    && let Some(source) = path.first()
                    && let [target] = binding.targets.as_slice()
                {
                    let design = designs
                        .get(source)
                        .cloned()
                        .unwrap_or_else(|| source.clone());
                    designs.insert(target.name.clone(), design);
                }
                // A judgement bound to a name is judged the same as one
                // written in a condition.
                judge(&binding.value, designs, required, lineage, found);
            }
            CheckedStatement::If {
                condition,
                body,
                else_body,
            } => {
                judge(condition, designs, required, lineage, found);
                collect_acceptance_calls(body, designs, required, lineage, found);
                collect_acceptance_calls(else_body, designs, required, lineage, found);
            }
            CheckedStatement::Match { cases, .. } => {
                for case in cases {
                    collect_acceptance_calls(&case.body, designs, required, lineage, found);
                }
            }
            CheckedStatement::For { body, .. } | CheckedStatement::When { body, .. } => {
                collect_acceptance_calls(body, designs, required, lineage, found);
            }
            _ => {}
        }
    }
}

fn judge(
    expression: &TypedExpression,
    designs: &std::collections::HashMap<String, String>,
    required: &std::collections::HashMap<&str, u64>,
    lineage: &crate::provenance::LineageMap,
    found: &mut Option<(String, u64, usize)>,
) {
    let CheckedExpression::Call {
        operation,
        arguments,
    } = &expression.value
    else {
        return;
    };
    // An acceptance judgement is registered as `<Type>.accepts`, taking the
    // design and its evidence, so the suffix names the judgement whichever
    // artifact type it is about.
    if !operation.ends_with(".accepts") || found.is_some() {
        return;
    }
    let [design, evidence] = arguments.as_slice() else {
        return;
    };
    let CheckedExpression::Reference { path, .. } = &design.value.value else {
        return;
    };
    let Some(name) = path.first() else {
        return;
    };
    let design = designs.get(name).cloned().unwrap_or_else(|| name.clone());
    let Some(asked) = required.get(design.as_str()).copied() else {
        return;
    };
    // Silence where the lineage is not fully known: a sample from a caller
    // could be anything, and guessing there would refuse correct programs.
    let Some(offered) = lineage.of(&evidence.value).independent_count() else {
        return;
    };
    if (offered as u64) < asked {
        *found = Some((design, asked, offered));
    }
}

/// The span of the first acceptance judgement in a workflow, so the diagnostic
/// points at the claim rather than at the file.
fn acceptance_call_span(module: &Module, workflow: &str) -> Option<Span> {
    let Item::Workflow(declaration) = module.items.iter().find(
        |item| matches!(item, Item::Workflow(candidate) if candidate.name.value == workflow),
    )?
    else {
        return None;
    };
    fn in_statements(statements: &[Stmt]) -> Option<Span> {
        statements.iter().find_map(in_statement)
    }
    fn in_statement(statement: &Stmt) -> Option<Span> {
        match statement {
            Stmt::If(value) => in_expression(&value.condition)
                .or_else(|| in_statements(&value.then_body))
                .or_else(|| in_statements(&value.else_body)),
            Stmt::Binding(value) => in_expression(&value.value),
            Stmt::For(value) => in_statements(&value.body),
            Stmt::When(value) => in_statements(&value.body),
            Stmt::Match(value) => in_expression(&value.value).or_else(|| {
                value.cases.iter().find_map(|case| {
                    case.guard
                        .as_ref()
                        .and_then(in_expression)
                        .or_else(|| in_statements(&case.body))
                })
            }),
            _ => None,
        }
    }
    fn in_expression(expression: &Expr) -> Option<Span> {
        match expression {
            Expr::Call { callee, span, .. } => match callee.as_ref() {
                Expr::Path(path)
                    if path
                        .segments
                        .last()
                        .is_some_and(|segment| segment.value == "accepts") =>
                {
                    Some(*span)
                }
                _ => None,
            },
            Expr::Unary { operand, .. } => in_expression(operand),
            Expr::Binary { left, right, .. } => {
                in_expression(left).or_else(|| in_expression(right))
            }
            _ => None,
        }
    }
    in_statements(&declaration.body)
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
            "../../../../docs/language/specimens/plasmid-design.lab"
        ))
        .unwrap();
        assert!(module.declarations.iter().any(
            |declaration| matches!(declaration, CheckedDeclaration::Artifact { artifact, .. } if artifact == "plasmid")
        ));
    }

    /// Grounding is ordinary role membership, so a kind resolves to the terms
    /// of every role it plays and a compact identifier reaches the checked IR
    /// already expanded.
    #[test]
    fn a_grounded_kind_resolves_to_its_ontology_terms() {
        let module = compile_module(concat!(
            "role EngineeredRegion = \"SO:0000804\"\n",
            "role NucleicAcid = \"https://identifiers.org/SBO:0000251\"\n",
            "\n",
            "artifact Plasmid is EngineeredRegion, NucleicAcid\n",
        ))
        .expect("a grounded kind compiles");

        let term = module
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                CheckedDeclaration::Role {
                    name,
                    term: Some(term),
                    ..
                } if name == "EngineeredRegion" => Some(term.clone()),
                _ => None,
            })
            .expect("the role carries its term");
        assert_eq!(term, "https://identifiers.org/SO:0000804");

        let roles = module
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                CheckedDeclaration::ArtifactKind { name, roles, .. } if name == "plasmid" => {
                    Some(roles.clone())
                }
                _ => None,
            })
            .expect("the kind carries its roles");
        assert_eq!(
            roles,
            vec!["EngineeredRegion".to_owned(), "NucleicAcid".to_owned()]
        );
    }

    /// A role's term is part of its public surface. Without it an importing
    /// module could satisfy a bound and still not know what the type is.
    #[test]
    fn grounding_survives_an_import() {
        let mut environment = SemanticEnvironment::default();
        let terms = compile_module_with_id(
            ModuleId::new("vocab.so"),
            "role EngineeredRegion = \"SO:0000804\"\n",
        )
        .expect("the vocabulary compiles");
        environment.insert("vocab.so", terms.interface.clone());

        let designs = compile_module_in_environment(
            ModuleId::new("designs"),
            "use vocab.so\n\nartifact Plasmid is EngineeredRegion\n",
            &environment,
        )
        .expect("a kind grounded in an imported role compiles");

        assert_eq!(
            designs.interface.exports["plasmid"].roles,
            vec!["EngineeredRegion".to_owned()]
        );
        assert_eq!(
            terms.interface.exports["EngineeredRegion"].term.as_deref(),
            Some("https://identifiers.org/SO:0000804")
        );
    }

    /// A kind may only be grounded in a role that exists, the same rule a
    /// record's `is` clause follows.
    #[test]
    fn rejects_a_kind_grounded_in_an_undeclared_role() {
        let error = compile_module("artifact Plasmid is EngineeredRegion\n")
            .expect_err("'EngineeredRegion' is not declared");
        let ModuleError::Semantic(error) = error else {
            panic!("expected a semantic error, found {error:?}");
        };
        assert!(error.message.contains("EngineeredRegion"), "{error:?}");
    }

    /// The term is checked where it is written rather than when a document is
    /// emitted, so a typo names the line that made it.
    #[test]
    fn rejects_a_malformed_ontology_term() {
        let error = compile_module("role EngineeredRegion = \"engineered region\"\n")
            .expect_err("'engineered region' is not a term");
        let ModuleError::Semantic(error) = error else {
            panic!("expected a semantic error, found {error:?}");
        };
        assert!(
            error.message.contains("neither an IRI nor a compact"),
            "{error:?}"
        );
    }

    /// A role that names no term still classifies types. Grounding is optional,
    /// so every existing role keeps working unchanged.
    #[test]
    fn an_ungrounded_role_carries_no_term() {
        let module = compile_module("role Inducible\n").expect("an ungrounded role compiles");
        assert!(module.declarations.iter().any(|declaration| matches!(
            declaration,
            CheckedDeclaration::Role { name, term: None, .. } if name == "Inducible"
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
            "../../../../docs/language/specimens/plasmid-build.lab"
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
            include_str!("../../../../examples/golden-gate/src/designs/inventory.lab"),
        ),
        (
            "golden_gate.designs.plasmids",
            include_str!("../../../../examples/golden-gate/src/designs/plasmids.lab"),
        ),
        (
            "golden_gate.designs.strains",
            include_str!("../../../../examples/golden-gate/src/designs/strains.lab"),
        ),
        (
            "golden_gate.workflows.assemble",
            include_str!("../../../../examples/golden-gate/src/workflows/assemble.lab"),
        ),
        (
            "golden_gate.workflows.build_strains",
            include_str!("../../../../examples/golden-gate/src/workflows/build_strains.lab"),
        ),
        (
            "golden_gate.programs.reporter_panel",
            include_str!("../../../../examples/golden-gate/src/programs/reporter_panel.lab"),
        ),
    ];

    #[test]
    fn documentation_travels_in_the_checked_module_and_its_interface() {
        let module = compile_module(
            "/*! Synthetic reporter designs. */\n\nuse std.bio.designs\nuse std.bio.golden_gate\n\n/** A synthetic reporter plasmid. */\nplasmid reporter:\n  sequence = dna(\"ACGT\")\n",
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
            CheckedDeclaration::Artifact { artifact, .. } if artifact == "plasmid"
        )));
        assert!(declarations.iter().any(|declaration| matches!(
            declaration,
            CheckedDeclaration::Artifact { artifact, .. } if artifact == "strain"
        )));
        assert!(
            declarations
                .iter()
                .any(|declaration| matches!(declaration, CheckedDeclaration::Workflow { .. }))
        );

        // A component list names inventory identities imported from another
        // module, and stays a structured list of references rather than
        // collapsing into strings.
        //
        // Each element keeps the kind its catalogue entry was declared with, so
        // the list says a promoter drives a coding sequence rather than
        // flattening every element to the one kind they have in common.
        let components = declarations
            .iter()
            .find_map(|declaration| {
                let CheckedDeclaration::Artifact {
                    name, properties, ..
                } = declaration
                else {
                    return None;
                };
                (name == "GVD0011").then(|| {
                    properties
                        .iter()
                        .find(|property| property.name == "components")
                        .unwrap()
                })
            })
            .unwrap();
        assert_eq!(
            components.value.r#type.display_name(),
            "List<Promoter | Part | CDS>"
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

    /// A composite assembly can carry an already-assembled plasmid alongside
    /// bare parts, which the Golden Gate example has no occasion to do.
    #[test]
    fn a_component_list_admits_both_plasmids_and_parts() {
        let module = compile_module(
            r#"use std.bio.designs
use std.bio.golden_gate

buy backbone pSB1C3
buy restriction_enzyme BsaI
buy part J23101
buy part GFP

plasmid promoter_carrier:
  sequence = dna("GCTAGCGGATCCATGACCATGATTACGCCAAGCTTGAATTC")
  backbone = pSB1C3
  components = [J23101]
  restriction_enzyme = BsaI
  require topology == circular
  accept sequence == design.sequence

plasmid reporter_region:
  sequence = dna("GATCCTCTAGAGTCGACCTGCAGGCATGCAAGCTTGGCACT")
  backbone = pSB1C3
  components = [promoter_carrier, GFP]
  restriction_enzyme = BsaI
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
            r#"use std.bio.designs

workflow preserve(
  product: Material<Plasmid>,
  plate: Material<Medium is inoculated>,
) -> (
  product: Material<Plasmid>,
  plate: Material<Medium is inoculated>,
):
  return product, plate

workflow delegate(
  product: Material<Plasmid>,
  plate: Material<Medium is inoculated>,
) -> (
  product: Material<Plasmid>,
  plate: Material<Medium is inoculated>,
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

    /// `record` is the one declaration word for structured data; what a record
    /// is for is a role it plays, and each role is something the checker reads:
    /// `Event` is what `emit` resolves against, and `Evidential` is what may
    /// support a claim.
    #[test]
    fn a_records_roles_say_what_the_checker_reads_from_it() {
        let module = compile_module(
            "record Started is Event\n\nrecord PlateReading is Evidential:\n  count: Integer\n\nrecord Aliquot:\n  volume: Integer\n",
        )
        .expect("one declaration word, with roles saying what each type is for");

        let roles = module
            .declarations
            .iter()
            .filter_map(|declaration| match declaration {
                CheckedDeclaration::Data { name, roles, .. } => {
                    Some((name.as_str(), roles.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            roles,
            [
                ("Started", vec!["Event".to_owned()]),
                ("PlateReading", vec!["Evidential".to_owned()]),
                ("Aliquot", Vec::new()),
            ]
        );
    }

    /// `observation` is an ordinary identifier, not a declaration word, so a
    /// line led by it parses as an artifact instance of an unknown kind.
    #[test]
    fn only_record_opens_a_data_declaration() {
        let error = compile_module("observation PlateReading:\n  count: Integer\n").unwrap_err();
        assert!(
            error.to_string().contains("expected"),
            "'observation' does not open a declaration: {error}"
        );
    }

    /// What an order names is the declared name unless the item states a
    /// supplier identity. This is unrelated to its SBOL Component IRI.
    #[test]
    fn a_bought_item_defaults_its_supplier_identity_to_its_name() {
        let module = compile_module(
            "use std.bio.designs\nuse std.bio.golden_gate\n\nbuy part J23101\nbuy part GFP\nbuy restriction_enzyme BsaI_HF:\n  supplier_identity = \"BsaI-HF-v2\"\n",
        )
        .expect("each bought item names its kind");

        let catalogued = module
            .declarations
            .iter()
            .filter_map(|declaration| match declaration {
                CheckedDeclaration::Catalog {
                    name,
                    supplier_identity,
                    r#type,
                    ..
                } => Some((
                    name.as_str(),
                    supplier_identity.as_str(),
                    r#type.display_name(),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            catalogued,
            [
                ("J23101", "J23101", "Part".to_owned()),
                ("GFP", "GFP", "Part".to_owned()),
                ("BsaI_HF", "BsaI-HF-v2", "RestrictionEnzyme".to_owned()),
            ],
            "each name is its own declaration, and an order identifier is written only where it differs"
        );
    }

    #[test]
    fn legacy_identity_remains_a_supplier_identity_alias() {
        let module =
            compile_module("use std.bio.designs\n\nbuy part legacy:\n  identity = \"SKU-17\"\n")
                .unwrap();
        let CheckedDeclaration::Catalog {
            sbol_identity,
            supplier_identity,
            properties,
            ..
        } = &module.declarations[0]
        else {
            panic!("the declaration is bought");
        };
        assert_eq!(sbol_identity, &None);
        assert_eq!(supplier_identity, "SKU-17");
        assert!(properties.is_empty());
    }

    #[test]
    fn sbol_identity_is_preserved_independently_of_build_or_buy() {
        let module = compile_module(
            r#"use std.bio.designs

build part local_design:
  sbol_identity = "https://example.org/design/local"

buy part catalogued_design:
  sbol_identity = "https://example.org/design/catalogued"
  supplier_identity = "SKU-42"
"#,
        )
        .unwrap();

        let CheckedDeclaration::Artifact { sbol_identity, .. } = &module.declarations[0] else {
            panic!("the first declaration is built");
        };
        assert_eq!(
            sbol_identity.as_deref(),
            Some("https://example.org/design/local")
        );
        let CheckedDeclaration::Catalog {
            sbol_identity,
            supplier_identity,
            ..
        } = &module.declarations[1]
        else {
            panic!("the second declaration is bought");
        };
        assert_eq!(
            sbol_identity.as_deref(),
            Some("https://example.org/design/catalogued")
        );
        assert_eq!(supplier_identity, "SKU-42");
    }

    #[test]
    fn sbol_identity_must_be_an_absolute_iri() {
        let error = compile_module(
            "use std.bio.designs\n\nbuy part local:\n  sbol_identity = \"BBa_J23101\"\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("not an absolute IRI"), "{error}");
    }

    /// A parameterized type is catalogued when its head is, which is a separate
    /// question from whether the whole type packs into `any Role`.
    #[test]
    fn a_parameterized_catalogued_type_may_be_declared_by_name() {
        let module = compile_module(
            "use std.bio.designs\nuse std.bio.golden_gate\nuse std.bio.parts\n\nbuy promoter pLac: Promoter<Tetracycline>\n",
        )
        .expect("a promoter of anything is catalogued");
        assert!(module.declarations.iter().any(|declaration| matches!(
            declaration,
            CheckedDeclaration::Catalog { name, .. } if name == "pLac"
        )));
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
use std.bio.designs
use std.bio.golden_gate

buy part J23101
buy backbone pSB1C3
buy restriction_enzyme BsaI

plasmid p_gfp:
  sequence = dna("ACGT")
  backbone = pSB1C3
  components = [J23101]
  restriction_enzyme = BsaI
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
            r#"use std.lab.plasmid
use std.bio.designs
use std.bio.golden_gate

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
    fn lowers_action_intent_ownership_and_result_contract() {
        let module = compile_module(
            r#"use std.lab.plasmid
use std.bio.designs
use std.bio.golden_gate

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
        assert_eq!(module.schema_version, "lab.portable-module.v11");
        assert_eq!(action.operation, "std.lab.plasmid.store");
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

    /// A catalogued item states what its type says it states — the enzyme
    /// carries its own working temperature rather than every design repeating
    /// it.
    const ENZYME: &str = r#"record Enzyme:
  digest_temperature: Integer
  supplier: String

artifact Enzyme

"#;

    #[test]
    fn a_catalogued_item_states_the_fields_of_its_type() {
        let module = compile_module(&format!(
            "{ENZYME}buy enzyme BsaI:\n  digest_temperature = 37\n  supplier = \"NEB\"\n"
        ))
        .unwrap();

        let catalogued = module
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                CheckedDeclaration::Catalog {
                    name, properties, ..
                } if name == "BsaI" => Some(properties),
                _ => None,
            })
            .expect("the catalogued enzyme is declared");
        assert_eq!(catalogued.len(), 2);
    }

    #[test]
    fn a_catalogued_item_states_every_field_of_its_type() {
        let error = compile_module(&format!(
            "{ENZYME}buy enzyme BsaI:\n  digest_temperature = 37\n"
        ))
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("'BsaI' does not state 'supplier'"),
            "the type's fields are the item's schema: {error}"
        );
    }

    #[test]
    fn rejects_a_catalogued_property_its_type_does_not_declare() {
        let error = compile_module(&format!(
            "{ENZYME}buy enzyme BsaI:\n  digest_temp = 37\n  supplier = \"NEB\"\n"
        ))
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Enzyme has no property 'digest_temp'"),
            "an undeclared property is a mistake, not an extension: {error}"
        );
    }

    /// A supplier's item is never built here, so nothing about building it —
    /// a claim, or the evidentiary standard claims are believed on — may be
    /// stated on it.
    #[test]
    fn rejects_build_facts_on_a_bought_item() {
        for member in [
            "accept sequence == dna(\"ACGT\")",
            "across 3 biological replicates",
        ] {
            let error = compile_module(&format!(
                "use std.bio.designs\nuse std.bio.golden_gate\n\nbuy plasmid carrier:\n  sequence = dna(\"ACGT\")\n  {member}\n"
            ))
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("is bought, so nothing here builds it"),
                "'{member}' describes a thing a laboratory makes: {error}"
            );
        }
    }

    /// A declaration sets the standard its claims are believed on, and a claim
    /// may set its own instead. Three measurements of one colony are one
    /// biological replicate, so this counts entities rather than measurements.
    #[test]
    fn a_claim_takes_the_declarations_evidence_unless_it_states_its_own() {
        let module = compile_module(
            r#"use std.bio.designs
use std.bio.golden_gate

plasmid p_gfp:
  sequence = dna("ACGT")

  across 3 biological replicates

  accept concentration >= 100 ng/uL
  accept volume >= 20 uL across 1 biological replicate
"#,
        )
        .unwrap();

        let acceptance = module
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                CheckedDeclaration::Artifact { acceptance, .. } => Some(acceptance),
                _ => None,
            })
            .expect("the plasmid is declared");
        assert_eq!(
            acceptance[0].replicates,
            Some(3),
            "inherits the declaration"
        );
        assert_eq!(acceptance[1].replicates, Some(1), "states its own");
    }

    /// A standard written below the claims it governs still governs them, so
    /// what a claim is believed on does not depend on where the standard sits.
    #[test]
    fn a_declarations_evidence_reaches_claims_written_above_it() {
        let module = compile_module(
            r#"use std.bio.designs
use std.bio.golden_gate

plasmid p_gfp:
  sequence = dna("ACGT")

  accept volume >= 20 uL

  across 2 biological replicates
"#,
        )
        .unwrap();

        let acceptance = module
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                CheckedDeclaration::Artifact { acceptance, .. } => Some(acceptance),
                _ => None,
            })
            .expect("the plasmid is declared");
        assert_eq!(acceptance[0].replicates, Some(2));
    }

    #[test]
    fn rejects_a_declaration_stating_its_evidence_twice() {
        let error = compile_module(
            r#"use std.bio.designs
use std.bio.golden_gate

plasmid p_gfp:
  sequence = dna("ACGT")

  across 3 biological replicates
  across 2 biological replicates

  accept volume >= 20 uL
"#,
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("states its evidence twice"),
            "one declaration sets one standard: {error}"
        );
    }

    #[test]
    fn rejects_a_claim_believed_on_no_replicates() {
        let error = compile_module(
            r#"use std.bio.designs
use std.bio.golden_gate

plasmid p_gfp:
  sequence = dna("ACGT")

  accept volume >= 20 uL across 0 biological replicates
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("cannot be believed on zero biological replicates"),
            "asking for no evidence is a mistake, not a way to opt out: {error}"
        );
    }

    const REPORTER: &str = r#"use std.bio.designs
use std.bio.golden_gate
use std.bio.build
use std.lab.plasmid

plasmid p_reporter:
  sequence = dna("ACGT")

  accept volume >= 20 uL across 3 biological replicates

"#;

    /// Measuring one sample three times is one biological replicate, however
    /// many measurements are taken.
    #[test]
    fn refuses_evidence_that_is_one_entity_measured_repeatedly() {
        let error = compile_module(&format!(
            "{REPORTER}{}",
            r#"workflow measure() -> Material<Plasmid>:
  product <- realize p_reporter
  first <- quantify product
  second <- quantify product
  if accepts(p_reporter, [first, second]):
    return product
  return product
"#
        ))
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("accepted on 3 biological replicates, but this evidence spans 1"),
            "two measurements of one sample are one replicate: {error}"
        );
    }

    #[test]
    fn accepts_evidence_from_enough_independent_lineages() {
        compile_module(&format!(
            "{REPORTER}{}",
            r#"workflow measure() -> Material<Plasmid>:
  a <- realize p_reporter
  b <- realize p_reporter
  c <- realize p_reporter
  first <- quantify a
  second <- quantify b
  third <- quantify c
  if accepts(p_reporter, [first, second, third]):
    <- dispose b
    <- dispose c
    return a
  <- dispose b
  <- dispose c
  return a
"#
        ))
        .expect("three independent realizations are three biological replicates");
    }

    /// A judgement bound to a name is the same judgement as one written in a
    /// condition, so it is held to the same standard.
    #[test]
    fn refuses_short_evidence_in_a_bound_judgement() {
        let error = compile_module(&format!(
            "{REPORTER}{}",
            r#"workflow measure() -> Material<Plasmid>:
  product <- realize p_reporter
  first <- quantify product
  second <- quantify product
  ok = accepts(p_reporter, [first, second])
  return product
"#
        ))
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("accepted on 3 biological replicates, but this evidence spans 1"),
            "a bound judgement is still a judgement: {error}"
        );
    }

    /// A sample that arrived from a caller could be anything, so the check says
    /// nothing rather than refusing a program it cannot judge.
    #[test]
    fn says_nothing_where_provenance_is_not_known() {
        compile_module(&format!(
            "{REPORTER}{}",
            r#"workflow measure(product: Material<Plasmid>) -> Material<Plasmid>:
  first <- quantify product
  second <- quantify product
  if accepts(p_reporter, [first, second]):
    return product
  return product
"#
        ))
        .expect("an unknown lineage is not a known-bad one");
    }

    /// A binding renames a contract's result, so lineage has to follow the name
    /// the program bound rather than the one the contract declared.
    #[test]
    fn follows_a_result_bound_under_a_different_name() {
        let error = compile_module(&format!(
            "{REPORTER}{}",
            r#"workflow measure() -> Material<Plasmid>:
  product <- realize p_reporter
  renamed <- quantify product
  also <- quantify product
  if accepts(p_reporter, [renamed, also]):
    return product
  return product
"#
        ))
        .unwrap_err();

        assert!(
            error.to_string().contains("this evidence spans 1"),
            "the contract calls this result 'evidence'; the program does not: {error}"
        );
    }

    #[test]
    fn names_a_quantity_by_the_unit_it_is_measured_in() {
        compile_module(
            r#"record Reagent:
  digest_temperature: Quantity<C>
  concentration: Quantity<ng/uL>

artifact Reagent

buy reagent BsaI:
  digest_temperature = 37 C
  concentration = 100 ng/uL
"#,
        )
        .expect("a compound unit is written the same way in a type as in a value");
    }

    /// Naming the type must not change what is checked: a unit still has to
    /// match exactly, so microlitres and millilitres remain different types.
    #[test]
    fn a_quantity_type_still_checks_its_unit_exactly() {
        let error = compile_module(
            r#"record Reagent:
  volume: Quantity<uL>

artifact Reagent

buy reagent BsaI:
  volume = 20 mL
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("expects Quantity<uL>, found Quantity<mL>"),
            "a thousandfold error on the bench is not a conversion: {error}"
        );
    }

    /// A measurement composes with another, and the result measures something
    /// neither operand measured. That is what lets a recipe state concentrations
    /// once and scale to whatever batch is being made.
    mod dimensions {
        use super::*;

        const SCHEMA: &str = r#"record Recipe

artifact Recipe:
  tryptone?: Quantity<g>
  salt?: Quantity<any Concentration>
  buffer?: Quantity<any Molarity>
  either?: Quantity<any Concentration> | Quantity<any Molarity>
  bulk?: Quantity<any Mass>
  length?: Quantity<bp>
  volume?: Quantity<uL>

"#;

        fn stated(body: &str) -> Result<String, ModuleError> {
            let module = compile_module(&format!("{SCHEMA}build recipe LB:\n{body}"))?;
            let declaration = module
                .declarations
                .iter()
                .find_map(|declaration| match declaration {
                    CheckedDeclaration::Artifact { properties, .. } => properties.first(),
                    _ => None,
                })
                .expect("the property was checked");
            let CheckedExpression::Quantity { magnitude, unit } = &declaration.value.value else {
                panic!("a composed measurement is a measurement");
            };
            Ok(format!("{magnitude} {unit}"))
        }

        /// The headline: a recipe holds concentrations and a batch is a volume,
        /// so what to weigh out is their product.
        #[test]
        fn a_recipe_scales_to_a_batch() {
            assert_eq!(
                stated("  tryptone = 10 g/L * 500 mL\n").expect("a recipe scales"),
                "5 g"
            );
        }

        /// A concentration divides into a mass to give the volume holding it,
        /// which is the arithmetic behind every dilution done at a bench.
        #[test]
        fn a_mass_over_a_concentration_is_a_volume() {
            assert_eq!(
                stated("  volume = (500 ng / 100 ng/uL) in uL\n").expect("a dilution computes"),
                "5 uL"
            );
        }

        /// The result lands in the canonical unit of what it measures, so it is
        /// predictable rather than inherited from whichever operand came first.
        #[test]
        fn a_composed_measurement_lands_in_a_canonical_unit() {
            assert_eq!(
                stated("  bulk = 2 mg * 3\n").expect("scaling keeps its unit"),
                "6 mg"
            );
        }

        /// Conversion is written. `12 kb` is what a person means and `12000 bp`
        /// is what the field holds, and saying so is one word.
        #[test]
        fn a_measurement_converts_where_it_is_written() {
            assert_eq!(
                stated("  length = 12 kb in bp\n").expect("kilobases are base pairs"),
                "12000 bp"
            );
        }

        #[test]
        fn refuses_converting_between_different_things() {
            let error = stated("  length = 12 kb in uL\n").unwrap_err().to_string();
            assert!(
                error.contains("do not measure the same thing"),
                "a length is not a volume: {error}"
            );
        }

        /// A field naming a dimension takes any unit of it, so a recipe holds
        /// milligrams per litre beside grams per litre without pinning either.
        ///
        /// Mass in a volume and amount in a volume stay different things: going
        /// between them needs a molar mass, which is a fact about the substance
        /// and not about the recipe. A field that holds either says so.
        #[test]
        fn a_field_may_ask_for_a_dimension_rather_than_a_unit() {
            compile_module(&format!("{SCHEMA}build recipe LB:\n  salt = 10 g/L\n"))
                .expect("grams per litre is a concentration");
            compile_module(&format!("{SCHEMA}build recipe TE:\n  buffer = 50 mM\n"))
                .expect("millimolar is a molarity");
            compile_module(&format!("{SCHEMA}build recipe TE:\n  either = 50 mM\n"))
                .expect("a field holding either takes both");
            let error = compile_module(&format!("{SCHEMA}build recipe TE:\n  salt = 50 mM\n"))
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("expects Quantity<any Concentration>"),
                "mass in a volume is not amount in a volume: {error}"
            );

            let error = compile_module(&format!("{SCHEMA}build recipe LB:\n  bulk = 5 mL\n"))
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("expects Quantity<any Mass>"),
                "a volume is not a mass: {error}"
            );
        }

        #[test]
        fn refuses_a_dimension_this_compiler_does_not_measure() {
            let error = compile_module(
                "record R\n\nartifact R:\n  x?: Quantity<any Luminosity>\n\nbuild r a:\n  x = 1 cd\n",
            )
            .unwrap_err()
            .to_string();
            assert!(
                error.contains("'Luminosity' is not something this compiler measures"),
                "the diagnostic names what it does measure: {error}"
            );
        }
    }

    /// An assignment refused microlitres against millilitres while arithmetic
    /// and comparison let the same two units meet freely. Both halves of the
    /// language now hold the unit to the same standard.
    mod quantity_arithmetic {
        use super::*;

        fn body(body: &str) -> Result<CheckedModule, ModuleError> {
            compile_module(&format!(
                "artifact Plasmid:\n  a?: Quantity<uL>\n\nplasmid p:\n{body}"
            ))
        }

        fn refuses(source: &str) -> String {
            body(source)
                .expect_err("two units that are not the same unit cannot meet")
                .to_string()
        }

        #[test]
        fn measurements_in_one_unit_combine() {
            body("  a = 20 uL + 5 uL\n").expect("microlitres add to microlitres");
            body("  a = 20 uL - 5 uL\n").expect("microlitres subtract from microlitres");
            body("  a = 20 uL\n  require a > 5 uL\n").expect("one unit compares with itself");
        }

        /// Scaling a measurement by a count is how a recipe states a batch, and
        /// it keeps the unit it started in.
        #[test]
        fn a_measurement_scales_by_a_plain_number() {
            body("  a = 20 uL * 3\n").expect("a volume times a count is a volume");
            body("  a = 20 uL / 2\n").expect("a volume divided by a count is a volume");
        }

        /// A slash after a unit reads as a denominator, which made a quantity
        /// impossible to divide. A denominator is a unit, so anything else is
        /// division.
        #[test]
        fn a_compound_unit_still_reads_as_one_unit() {
            compile_module(
                "record Reagent:\n  concentration: Quantity<ng/uL>\n\nartifact Reagent\n\nbuy reagent stock:\n  concentration = 100 ng/uL\n",
            )
            .expect("a compound unit is not a division");
        }

        #[test]
        fn refuses_two_units_meeting_in_arithmetic() {
            for source in ["  a = 20 uL + 5 mL\n", "  a = 20 uL - 5 mL\n"] {
                let error = refuses(source);
                assert!(
                    error.contains("'uL' and 'mL' are different units"),
                    "the diagnostic names both units: {error}"
                );
            }
        }

        #[test]
        fn refuses_two_units_meeting_in_a_comparison() {
            for source in [
                "  a = 20 uL\n  require a > 5 mL\n",
                "  a = 20 uL\n  require a == 5 mL\n",
            ] {
                let error = refuses(source);
                assert!(
                    error.contains("'uL' and 'mL' are different units"),
                    "equality is no more askable across units than ordering is: {error}"
                );
            }
        }

        /// Two measurements multiplied give a quantity in neither operand's
        /// unit. A volume times a volume measures something a laboratory has no
        /// unit for, so there is nothing to write the answer in.
        #[test]
        fn refuses_a_product_with_no_unit_to_write_it_in() {
            let error = refuses("  a = 20 uL * 5 uL\n");
            assert!(
                error.contains("no unit to write it in"),
                "a volume times a volume is not a volume: {error}"
            );
        }

        /// A measurement divided by one measuring the same thing is a plain
        /// ratio, which is how a dilution factor is written.
        #[test]
        fn one_measurement_over_another_of_the_same_thing_is_a_number() {
            body("  a = 20 uL\n  require (100 uL / 20 uL) > 4.0\n")
                .expect("a ratio of volumes is a number");
        }
    }

    #[test]
    fn requires_every_field_a_schema_does_not_mark_optional() {
        let error = compile_module(
            r#"artifact Plasmid:
  label: String
  note?: String

plasmid sample:
  note = "thawed"
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("plasmid 'sample' does not state 'label'"),
            "a field without '?' is one every declaration states: {error}"
        );
    }

    #[test]
    fn accepts_a_declaration_that_omits_only_optional_fields() {
        compile_module(
            r#"artifact Plasmid:
  label: String
  note?: String

plasmid sample:
  label = "tube 1"
"#,
        )
        .expect("an optional field may go unstated");
    }

    #[test]
    fn rejects_a_rule_reading_a_property_this_declaration_omits() {
        let error = compile_module(
            r#"artifact Plasmid:
  label: String
  note?: String

plasmid sample:
  label = "tube 1"

  accept note == "thawed"
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("plasmid 'sample' does not state 'note'"),
            "an omitted optional is absent, not empty: {error}"
        );
    }

    #[test]
    fn a_rule_reads_the_produced_type_field_over_an_omitted_property() {
        // A rule constrains the artifact that gets built. `Plasmid` carries its
        // own `sequence`, so the rule reads the realized material's sequence
        // and does not care that the declaration stated none.
        compile_module(
            r#"artifact Plasmid:
  label: String
  sequence?: DNA

plasmid sample:
  label = "tube 1"

  accept sequence == dna("ACGT")
"#,
        )
        .expect("a rule reads the produced type's field");
    }

    /// A package declares a durable verb with `action`, and a workflow checks
    /// against it the way it checks a bundled one. A new verb is a declaration.
    mod actions {
        use super::*;

        const CENTRIFUGE: &str = r#"use std.bio.designs
use std.lab.plasmid

action centrifuge <culture> at <force> for <duration> -> pellet:
  culture: take Material<Strain is recovered>
  force: Quantity<rcf>
  duration: Quantity<min>
  pellet: Material<Strain is recovered>
  requires Centrifugation

buy chassis DH5alpha:
  competence = competent
  efficiency = 1e9 cfu/ug

buy plasmid p:
  sbol_identity = "https://example.org/p"
  sequence = dna("ACGT")

build strain s:
  chassis = DH5alpha
  plasmids = [p]

workflow spin(dna: List<Material<Plasmid>>) -> Material<Strain is recovered>:
  cells <- provision DH5alpha
  strain, culture <- transform s from dna into cells
  culture <- recover culture for 1 h
  pellet <- centrifuge culture at 4000 rcf for 10 min
  <- dispose strain
  return pellet
"#;

        #[test]
        fn a_declared_verb_checks_in_a_workflow() {
            let module = compile_module(CENTRIFUGE).expect("a declared verb is usable");
            let action = module
                .declarations
                .iter()
                .find_map(|declaration| match declaration {
                    CheckedDeclaration::Action {
                        name,
                        operation,
                        capability,
                        ..
                    } => Some((name.clone(), operation.clone(), capability.clone())),
                    _ => None,
                });
            let (name, operation, capability) = action.expect("the action was checked");
            assert_eq!(name, "centrifuge");
            assert_eq!(operation, "standalone.centrifuge");
            assert_eq!(capability, "Centrifugation");
        }

        /// The phrase is checked as written: a wrong word or a missing operand
        /// is a diagnostic against the declared shape.
        #[test]
        fn refuses_a_call_that_does_not_match_the_phrase() {
            let wrong = CENTRIFUGE.replace(
                "centrifuge culture at 4000 rcf for 10 min",
                "centrifuge culture at 4000 rcf",
            );
            let error = compile_module(&wrong).unwrap_err().to_string();
            assert!(
                error.contains("centrifuge"),
                "the phrase is checked against the declaration: {error}"
            );
        }

        /// A measurement operand pins its unit, so calling with another is the
        /// same thousandfold refusal a field gets.
        #[test]
        fn a_measurement_operand_checks_its_unit() {
            let wrong = CENTRIFUGE.replace("at 4000 rcf", "at 4000 g");
            let error = compile_module(&wrong).unwrap_err().to_string();
            assert!(
                error.contains("rcf"),
                "the operand's unit is what it accepts: {error}"
            );
        }

        /// A verb an importing module uses is checked against the same contract
        /// the declaring module wrote.
        #[test]
        fn a_verb_crosses_a_module_boundary() {
            let verbs = "use std.bio.designs

action chill <culture> for <duration> -> chilled:
  culture: take Material<Strain is recovered>
  duration: Quantity<min>
  chilled: Material<Strain is recovered>
  requires StaticIncubation
";
            let designs =
                compile_module_with_id(ModuleId::new("pkg.verbs"), verbs).expect("verbs compile");
            let mut environment = SemanticEnvironment::default();
            environment.insert("pkg.verbs", designs.interface.clone());
            compile_module_in_environment(
                ModuleId::new("pkg.work"),
                "use std.bio.designs
use std.lab.plasmid
use pkg.verbs

workflow w(c: Material<Strain is recovered>) -> Material<Strain is recovered>:
  c <- chill c for 20 min
  return c
",
                &environment,
            )
            .expect("an imported verb checks a workflow");
        }

        #[test]
        fn an_action_states_the_capability_it_needs() {
            let error = compile_module(
                "action spin <c> -> c:
  c: take Material<Plasmid>
",
            )
            .unwrap_err()
            .to_string();
            assert!(
                error.contains("capability"),
                "a verb without a capability cannot be allocated: {error}"
            );
        }
    }

    #[test]
    fn rejects_a_completeness_rule_naming_a_required_field() {
        let error = compile_module(
            r#"artifact Plasmid:
  label: String
  note?: String

  declares label or note
"#,
        )
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("'label' is required, so a completeness rule cannot mention it"),
            "requiredness and a completeness rule must not disagree: {error}"
        );
    }

    /// A facet classifies a kind's materials by the state they are in, which is
    /// what keeps a state off the type. `Culture` and `Plate` were types for
    /// want of this, and neither could name the design underneath it.
    mod facets {
        use super::*;
        use crate::ExportKind;

        const COMPETENCE: &str = r#"artifact Chassis:
  label?: String

facet Competence on Chassis:
  /** Cells as they come off an overnight culture. */
  naive
  /** Cells a transformation may be attempted in. */
  competent:
    efficiency: Quantity<cfu/ug>

  naive -> competent
"#;

        fn facet(module: &CheckedModule) -> (&CheckedType, &[CheckedFacetState]) {
            module
                .declarations
                .iter()
                .find_map(|declaration| match declaration {
                    CheckedDeclaration::Facet {
                        subject, states, ..
                    } => Some((subject, states.as_slice())),
                    _ => None,
                })
                .expect("the facet was checked")
        }

        #[test]
        fn a_facet_carries_its_subject_states_and_transitions() {
            let module = compile_module(COMPETENCE).expect("a facet over a declared kind checks");
            let (subject, states) = facet(&module);
            assert_eq!(subject.display_name(), "Chassis");
            assert_eq!(
                states
                    .iter()
                    .map(|state| state.name.as_str())
                    .collect::<Vec<_>>(),
                ["naive", "competent"],
                "declaration order is preserved, so the first state stays identifiable"
            );
            assert!(states[0].fields.is_empty());
            assert_eq!(states[1].fields[0].name, "efficiency");
            assert_eq!(
                states[1].fields[0].r#type.display_name(),
                "Quantity<cfu/ug>"
            );
            assert_eq!(
                states[1].doc.as_deref(),
                Some("Cells a transformation may be attempted in."),
                "a state documents itself, so the reference cannot drift from it"
            );
        }

        /// A facet is part of a module's public surface. An importer that cannot
        /// see the states cannot constrain a material to one.
        #[test]
        fn a_facet_is_exported_with_its_states() {
            let module = compile_module(COMPETENCE).expect("checks");
            let export = module
                .interface
                .exports
                .get("Competence")
                .expect("the facet is exported");
            assert_eq!(export.kind, ExportKind::Facet);
            let surface = export.facet.as_ref().expect("the states travel with it");
            assert_eq!(surface.subject.display_name(), "Chassis");
            assert_eq!(surface.states.len(), 2);
            assert_eq!(surface.transitions.len(), 1);
            assert_eq!(surface.transitions[0].from, "naive");
            assert_eq!(surface.transitions[0].to, "competent");
        }

        #[test]
        fn rejects_a_transition_naming_a_state_the_facet_has_not_declared() {
            let error = compile_module(
                "artifact Chassis\n\nfacet Competence on Chassis:\n  naive\n  competent\n\n  naive -> transformed\n",
            )
            .unwrap_err();
            let message = error.to_string();
            assert!(
                message.contains("facet 'Competence' has no state 'transformed'"),
                "the diagnostic names the facet and the missing state: {error}"
            );
        }

        /// A state nothing reaches cannot be established, so the kind is making a
        /// claim it cannot honor. The first state needs no transition into it
        /// because that is where a material starts.
        #[test]
        fn rejects_a_state_no_transition_reaches() {
            let error = compile_module(
                "artifact Chassis\n\nfacet Competence on Chassis:\n  naive\n  competent\n",
            )
            .unwrap_err();
            let message = error.to_string();
            assert!(
                message.contains("no transition reaches state 'competent'"),
                "an unreachable state is refused at its declaration: {error}"
            );
        }

        #[test]
        fn rejects_a_duplicate_state() {
            let error = compile_module(
                "artifact Chassis\n\nfacet Competence on Chassis:\n  naive\n  naive\n",
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("duplicate state 'naive'"),
                "each state a facet admits is listed once: {error}"
            );
        }

        /// Several facets may classify one kind and they stay independent. A
        /// culture that is both diluted and grown under selection is two facets,
        /// not one state naming both.
        #[test]
        fn a_kind_carries_several_independent_facets() {
            let module = compile_module(
                r#"record Broth

artifact Broth

facet Dilution on Broth:
  neat
  diluted

  neat -> diluted

facet Selection on Broth:
  permissive
  selective

  permissive -> selective
"#,
            )
            .expect("two facets over one kind check");
            let facets = module
                .declarations
                .iter()
                .filter(|declaration| matches!(declaration, CheckedDeclaration::Facet { .. }))
                .count();
            assert_eq!(facets, 2);
        }

        /// A facet resolves after every kind in the file, so a module reads with
        /// the thing first and the states it may be in after.
        #[test]
        fn a_facet_may_be_declared_above_the_kind_it_classifies() {
            compile_module(
                "facet Competence on Chassis:\n  naive\n  competent\n\n  naive -> competent\n\nartifact Chassis\n",
            )
            .expect("declaration order does not decide whether a facet resolves");
        }

        /// The first state is where a material starts, so it needs no transition
        /// into it and a facet naming only that state is complete.
        #[test]
        fn an_initial_state_needs_no_transition_into_it() {
            compile_module("artifact Chassis\n\nfacet Competence on Chassis:\n  naive\n")
                .expect("the state a material starts in is reachable by definition");
        }

        #[test]
        fn rejects_a_facet_on_something_that_is_not_a_kind() {
            let error = compile_module("facet Competence on Widget:\n  naive\n").unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("'Widget' is not a kind in scope"),
                "a facet classifies a kind that exists: {error}"
            );
        }

        /// A material narrowed to a state is written `Material<Chassis is
        /// competent>`. The state constrains the argument rather than wrapping
        /// the material, so ownership analysis still sees a material.
        mod narrowing {
            use super::*;

            const BASE: &str = r#"use std.lab.plasmid

artifact Chassis

facet Competence on Chassis:
  naive
  competent:
    efficiency: Quantity<cfu>

  naive -> competent
"#;

            fn check(tail: &str) -> Result<CheckedModule, ModuleError> {
                compile_module(&format!("{BASE}\n{tail}"))
            }

            /// Knowing which state a material is in is never a problem where any
            /// state is accepted. The reverse is what narrowing exists to
            /// refuse.
            #[test]
            fn narrowing_runs_one_way() {
                check("workflow w(c: Material<Chassis is competent>) -> Material<Chassis>:\n  return c\n")
                    .expect("a competent chassis is a chassis");

                let error =
                    check("workflow w(c: Material<Chassis>) -> Material<Chassis is competent>:\n  return c\n")
                        .expect_err("a chassis is not known to be competent");
                assert!(
                    error
                        .to_string()
                        .contains("expected Material<Chassis is competent>"),
                    "the diagnostic shows the state that was required: {error}"
                );
            }

            #[test]
            fn refuses_one_state_where_another_is_required() {
                let error = check(
                    "workflow w(c: Material<Chassis is naive>) -> Material<Chassis is competent>:\n  return c\n",
                )
                .expect_err("naive cells are not competent cells");
                assert!(
                    error.to_string().contains("Material<Chassis is naive>"),
                    "the diagnostic shows the state that was offered: {error}"
                );
            }

            /// The encoding matters. Wrapping the material instead of its
            /// argument would take it out of ownership analysis, which finds a
            /// material by its outermost name.
            #[test]
            fn a_narrowed_material_is_still_affine() {
                let error = check(
                    "workflow w(c: Material<Chassis is competent>) -> None:\n  <- dispose c\n  <- dispose c\n  return None\n",
                )
                .expect_err("narrowing a material does not launder its ownership");
                assert!(
                    error.to_string().contains("'c' is no longer available"),
                    "a narrowed material is consumed exactly like any other: {error}"
                );
            }

            #[test]
            fn rejects_a_state_no_facet_admits() {
                let error =
                    check("workflow w(c: Material<Chassis is transformed>) -> None:\n  <- dispose c\n  return None\n")
                        .expect_err("a state has to be one the kind declares");
                let message = error.to_string();
                assert!(
                    message.contains("'Chassis' has no state 'transformed'"),
                    "the diagnostic names the kind and the state: {error}"
                );
            }

            /// The whole point, end to end. `transform` requires competent
            /// cells; a chassis carries the state its declaration states; so
            /// transforming into cells nobody made competent is a diagnostic at
            /// the operand rather than a silent success.
            #[test]
            fn transformation_requires_cells_that_were_made_competent() {
                const PROGRAM: &str = r#"use std.bio.designs
use std.lab.plasmid

buy chassis DH5alpha:
  competence = competent
  efficiency = 1e9 cfu/ug

buy chassis Naive:
  competence = naive

buy plasmid p:
  sbol_identity = "https://example.org/p"
  sequence = dna("ACGT")

build strain s:
  chassis = DH5alpha
  plasmids = [p]

workflow build(dna: List<Material<Plasmid>>) -> Material<Strain>:
  cells <- provision {host}
  strain, culture <- transform s from dna into cells
  <- dispose culture
  return strain
"#;
                compile_module(&PROGRAM.replace("{host}", "DH5alpha"))
                    .expect("cells declared competent may be transformed");

                let error = compile_module(&PROGRAM.replace("{host}", "Naive"))
                    .expect_err("naive cells take up nothing");
                assert!(
                    error
                        .to_string()
                        .contains("expects Material<Chassis is competent>"),
                    "the operand names the state it required: {error}"
                );
            }

            #[test]
            fn rejects_narrowing_a_kind_no_facet_classifies() {
                let error = compile_module(
                    "use std.lab.plasmid\n\nrecord Widget\n\nartifact Widget\n\nworkflow w(c: Material<Widget is competent>) -> None:\n  <- dispose c\n  return None\n",
                )
                .expect_err("a kind with no facet has no states to narrow to");
                assert!(
                    error
                        .to_string()
                        .contains("'Widget' has no state 'competent'"),
                    "the diagnostic names the kind: {error}"
                );
            }
        }
    }
}
