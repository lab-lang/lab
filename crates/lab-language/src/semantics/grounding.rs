//! What ontology terms a Lab type stands for.
//!
//! Grounding is ordinary role membership: a role may name a term, a type plays
//! roles, and the terms of the roles it plays are what a document states about
//! it. Both halves are already part of a module's public surface, so this joins
//! them rather than introducing a channel of its own.
//!
//! A consumer builds one index over every module in scope and asks it about a
//! type. Reading a single module is not enough: the role usually comes from a
//! vocabulary package and the membership from a design package, and neither
//! knows the whole answer alone.

use std::collections::{BTreeMap, BTreeSet};

use crate::checked::{CheckedDeclaration, CheckedModule, CheckedType};
use crate::semantics::{ExportKind, ModuleInterface};

/// The terms every type in scope stands for.
///
/// Ordered so that a document built from it lists the same terms in the same
/// order on every build; a diff between two builds should be a change in the
/// design, never a change in iteration order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grounding {
    /// The term each grounded role names.
    role_terms: BTreeMap<String, String>,
    /// The roles each type plays.
    type_roles: BTreeMap<String, BTreeSet<String>>,
    /// Which module declares the kind that produces each type.
    ///
    /// A design read from a document names kinds without saying where they come
    /// from, because a document states what its components are and not which
    /// Lab package describes them. Recording this is what lets such a module
    /// import exactly the packages the kinds it used are declared in, rather
    /// than being told or guessing.
    kind_modules: BTreeMap<String, String>,
}

impl Grounding {
    /// An index already holding the standard library's own grounding.
    ///
    /// Every program imports some of `std`, and the terms its kinds stand for
    /// are stated there rather than in the program, so starting empty would
    /// make the common case answer nothing.
    pub fn bundled() -> Self {
        let mut grounding = Self::default();
        for interface in crate::standard_library::authored_interfaces().values() {
            grounding.add_interface(interface);
        }
        grounding
    }

    /// Adds everything one interface publishes.
    ///
    /// Call this for every module in scope, including the standard library,
    /// before asking any question.
    pub fn add_interface(&mut self, interface: &ModuleInterface) {
        for (name, export) in &interface.exports {
            match export.kind {
                ExportKind::Role => {
                    if let Some(term) = &export.term {
                        self.role_terms.insert(name.clone(), term.clone());
                    }
                }
                // A kind's roles classify the type it produces, which is the
                // type a design names and a document is written about.
                ExportKind::ArtifactKind => {
                    let Some(schema) = &export.schema else {
                        continue;
                    };
                    if let CheckedType::Named { name, .. } = &schema.produces {
                        self.extend_roles(name, &export.roles);
                        // Several packages may describe one kind, and importing
                        // any of them puts the word in scope, so the first that
                        // declares it is enough to reach it by.
                        self.kind_modules
                            .entry(name.clone())
                            .or_insert_with(|| interface.module.to_string());
                    }
                }
                ExportKind::Type => self.extend_roles(name, &export.roles),
                _ => {}
            }
        }
    }

    /// Adds the declarations of a module being compiled, whose own grounding is
    /// not visible through an interface it has not published yet.
    pub fn add_module(&mut self, module: &CheckedModule) {
        for declaration in &module.declarations {
            match declaration {
                CheckedDeclaration::Role {
                    name,
                    term: Some(term),
                    ..
                } => {
                    self.role_terms.insert(name.clone(), term.clone());
                }
                CheckedDeclaration::ArtifactKind {
                    produces: CheckedType::Named { name, .. },
                    roles,
                    ..
                } => self.extend_roles(name, roles),
                CheckedDeclaration::Data { name, roles, .. } => self.extend_roles(name, roles),
                _ => {}
            }
        }
    }

    fn extend_roles(&mut self, ty: &str, roles: &[String]) {
        if roles.is_empty() {
            return;
        }
        self.type_roles
            .entry(ty.to_owned())
            .or_default()
            .extend(roles.iter().cloned());
    }

    /// The terms a type stands for.
    ///
    /// A parameterized type is grounded by its head: `Promoter<Tetracycline>`
    /// is a promoter whatever it responds to. A type that plays no grounded
    /// role yields nothing, which a caller must treat as "not stated" rather
    /// than as a claim that the type is uncharacterized.
    pub fn terms(&self, ty: &CheckedType) -> BTreeSet<String> {
        let CheckedType::Named { name, .. } = ty else {
            return BTreeSet::new();
        };
        self.terms_for_name(name)
    }

    /// The terms the type named `name` stands for.
    pub fn terms_for_name(&self, name: &str) -> BTreeSet<String> {
        self.type_roles
            .get(name)
            .into_iter()
            .flatten()
            .filter_map(|role| self.role_terms.get(role).cloned())
            .collect()
    }

    /// The term a role names, where it names one.
    pub fn role_term(&self, role: &str) -> Option<&str> {
        self.role_terms.get(role).map(String::as_str)
    }

    /// The module declaring the kind that produces `type_name`, which is what a
    /// module using that kind has to import to reach it.
    pub fn module_declaring(&self, type_name: &str) -> Option<&str> {
        self.kind_modules.get(type_name).map(String::as_str)
    }

    /// Every type that stands for at least one term, with the terms it stands
    /// for. This is what a reader inverts to recognize a type from a document.
    pub fn grounded_types(&self) -> impl Iterator<Item = (&str, BTreeSet<String>)> {
        self.type_roles.keys().filter_map(|name| {
            let terms = self.terms_for_name(name);
            (!terms.is_empty()).then_some((name.as_str(), terms))
        })
    }

    /// Whether any role in scope names a term. A program that grounds nothing
    /// is a program an SBOL emitter has nothing to say about.
    pub fn is_empty(&self) -> bool {
        self.role_terms.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::semantics::{ModuleId, SemanticEnvironment};
    use crate::{compile_module_in_environment, compile_module_with_id};

    /// The bundled standard library grounds its own kinds, so a program that
    /// imports `std.bio.designs` and states nothing about ontologies still
    /// describes its plasmids in terms an SBOL tool reads.
    #[test]
    fn the_standard_library_grounds_its_design_kinds() {
        let module = compile_module_with_id(
            ModuleId::new("designs"),
            "use std.bio.designs\n\nbuild plasmid p:\n  sequence = dna(\"ACGT\")\n",
        )
        .expect("the module compiles");

        let mut grounding = Grounding::bundled();
        grounding.add_module(&module);

        assert_eq!(
            grounding.terms_for_name("Plasmid"),
            BTreeSet::from([
                "https://identifiers.org/SBO:0000251".to_owned(),
                "https://identifiers.org/SO:0000804".to_owned(),
            ])
        );
        assert_eq!(
            grounding.terms_for_name("Antibiotic"),
            BTreeSet::from(["https://identifiers.org/SBO:0000247".to_owned()])
        );
    }

    /// A vocabulary package and a design package each hold half the answer, so
    /// the index must span both.
    #[test]
    fn grounding_joins_a_role_and_a_membership_from_two_modules() {
        let mut environment = SemanticEnvironment::default();
        let vocabulary = compile_module_with_id(
            ModuleId::new("vocab"),
            "role EngineeredRegion = \"SO:0000804\"\n",
        )
        .expect("the vocabulary compiles");
        environment.insert("vocab", vocabulary.interface.clone());

        let designs = compile_module_in_environment(
            ModuleId::new("designs"),
            "use vocab\n\nartifact Plasmid is EngineeredRegion\n",
            &environment,
        )
        .expect("the designs compile");

        let mut grounding = Grounding::default();
        grounding.add_interface(&vocabulary.interface);
        grounding.add_interface(&designs.interface);

        assert_eq!(
            grounding.terms_for_name("Plasmid"),
            BTreeSet::from(["https://identifiers.org/SO:0000804".to_owned()])
        );
    }

    /// An ungrounded type is silent rather than wrong. Nothing is claimed about
    /// a type whose roles name no term.
    #[test]
    fn an_ungrounded_type_yields_no_terms() {
        let module = compile_module_with_id(ModuleId::new("m"), "role Inducible\n")
            .expect("the module compiles");
        let mut grounding = Grounding::default();
        grounding.add_module(&module);
        assert!(grounding.terms_for_name("Inducible").is_empty());
        assert!(grounding.is_empty());
    }
}
