//! Where a material came from, and which materials are independent of it.
//!
//! Two samples can differ in a way no property of either one records. Three
//! colonies picked from a plate are independent transformants; one culture
//! split into three tubes is a single organism measured three times. The first
//! measures biological variance and the second measures pipetting variance,
//! and treating the second as though it were the first — pseudo-replication —
//! inflates a result's significance.
//!
//! Nothing about a sample says which it is. The answer is in where it came
//! from, so it is recovered by walking the dataflow a workflow already states:
//! actions declare their operands and results, and a result either begins a
//! lineage of its own or carries on the lineage of what went into it.

use std::collections::{BTreeMap, BTreeSet};

use crate::checked::{
    CheckedDeclaration, CheckedExpression, CheckedField, CheckedModule, CheckedStatement,
    CheckedType, ResolvedAction, TypedExpression,
};
use crate::standard_library::{Lineage, StandardLibrary};

/// One lineage beginning: a particular transformation, a particular colony
/// pick. Materials sharing an origin are the same biological entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Origin(usize);

/// Which lineage beginnings a material descends from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Provenance {
    /// Nothing here says where this came from. A workflow parameter arrives
    /// from a caller this analysis cannot see, so a check reading it must stay
    /// silent rather than guess.
    Unknown,
    /// Descends from exactly these beginnings. One origin means one biological
    /// entity, however many times it has been split or diluted.
    From(BTreeSet<Origin>),
    /// A collection whose members each descend from a beginning of their own,
    /// all established by one event — the colonies of a single pick.
    EachFrom(Origin),
}

impl Provenance {
    /// How many independent biological entities this stands for, when that is
    /// knowable. A collection whose size is a runtime value is not.
    pub fn independent_count(&self) -> Option<usize> {
        match self {
            Self::Unknown | Self::EachFrom(_) => None,
            Self::From(origins) => Some(origins.len()),
        }
    }

    fn origins(&self) -> Option<&BTreeSet<Origin>> {
        match self {
            Self::From(origins) => Some(origins),
            _ => None,
        }
    }
}

/// What every material bound in one workflow descends from.
#[derive(Clone, Debug, Default)]
pub struct LineageMap {
    bindings: BTreeMap<String, Provenance>,
}

impl LineageMap {
    pub fn get(&self, name: &str) -> Option<&Provenance> {
        self.bindings.get(name)
    }

    /// The provenance of an expression, which is the only question a check
    /// asks: a name refers to what it was bound to, a list to everything in it.
    pub fn of(&self, expression: &TypedExpression) -> Provenance {
        match &expression.value {
            CheckedExpression::Reference { path, .. } => path
                .first()
                .and_then(|name| self.bindings.get(name))
                .cloned()
                .unwrap_or(Provenance::Unknown),
            CheckedExpression::List { elements } => {
                let mut origins = BTreeSet::new();
                for element in elements {
                    match self.of(element) {
                        Provenance::From(from) => origins.extend(from),
                        // One member of unknown provenance makes the whole
                        // collection unknown: the check cannot say how many
                        // entities it spans without knowing about that one.
                        _ => return Provenance::Unknown,
                    }
                }
                Provenance::From(origins)
            }
            _ => Provenance::Unknown,
        }
    }
}

/// What each of an operation's results does to a lineage, in declaration order.
///
/// Positional rather than keyed by name, because a binding renames a result:
/// `evidence <- quantify sample` and `first <- quantify sample` bind the same
/// contract result to different names.
type LineageTable = BTreeMap<String, Vec<Lineage>>;

/// What every workflow in a module knows about where its materials came from,
/// keyed by workflow name.
///
/// A target reads this to decide what it may pool, and the language server
/// reads it to explain a sample's history where it is written.
pub fn lineage(module: &CheckedModule) -> BTreeMap<String, LineageMap> {
    let table = lineage_table(&StandardLibrary::bundled());
    module
        .declarations
        .iter()
        .filter_map(|declaration| match declaration {
            CheckedDeclaration::Workflow { name, body, .. } => {
                Some((name.clone(), analyze(body, &table)))
            }
            _ => None,
        })
        .collect()
}

/// Read the lineage each standard action declares for its results.
pub(crate) fn lineage_table(library: &StandardLibrary) -> LineageTable {
    library
        .action_specs()
        .map(|action| {
            let results = action.results.iter().map(|result| result.lineage).collect();
            (action.operation.to_owned(), results)
        })
        .collect()
}

/// Walk a workflow body, recording what each bound material descends from.
pub(crate) fn analyze(body: &[CheckedStatement], table: &LineageTable) -> LineageMap {
    let mut analyzer = Analyzer {
        table,
        next: 0,
        map: LineageMap::default(),
    };
    analyzer.block(body);
    analyzer.map
}

struct Analyzer<'a> {
    table: &'a LineageTable,
    next: usize,
    map: LineageMap,
}

impl Analyzer<'_> {
    fn block(&mut self, statements: &[CheckedStatement]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &CheckedStatement) {
        match statement {
            CheckedStatement::Binding(binding) => {
                let provenance = self.map.of(&binding.value);
                for target in &binding.targets {
                    self.map
                        .bindings
                        .insert(target.name.clone(), provenance.clone());
                }
            }
            CheckedStatement::Effect { results, action } => self.effect(results, action),
            CheckedStatement::If {
                body, else_body, ..
            } => {
                // A material bound on one branch is not in scope after the
                // other, so the branches are walked for their own bindings and
                // neither is merged into the other.
                self.block(body);
                self.block(else_body);
            }
            CheckedStatement::Match { cases, .. } => {
                for case in cases {
                    self.block(&case.body);
                }
            }
            CheckedStatement::For { binding, body, .. } => {
                // Which member of a family an iteration holds is a runtime
                // fact, so the variable stands for one member without saying
                // which. Nothing may then conclude that two iterations are the
                // same entity or different ones.
                self.map
                    .bindings
                    .insert(binding.name.clone(), Provenance::Unknown);
                self.block(body);
            }
            CheckedStatement::When { body, .. } => self.block(body),
            CheckedStatement::StateUpdate { .. }
            | CheckedStatement::Return { .. }
            | CheckedStatement::Emit { .. } => {}
        }
    }

    fn effect(&mut self, results: &[CheckedField], action: &ResolvedAction) {
        let declared = self.table.get(&action.operation);
        // What a continuing result carries on: the origins of every material
        // that went into the action.
        let mut inherited = BTreeSet::new();
        let mut any_unknown = false;
        for argument in &action.arguments {
            if !mentions_material(&argument.value.r#type) {
                continue;
            }
            match self.map.of(&argument.value) {
                Provenance::From(origins) => inherited.extend(origins),
                // A family's size is a runtime value, so what continues from
                // one spans an unknown number of entities — not one. Counting
                // it as one would refuse a program that measured genuinely
                // independent colonies.
                Provenance::EachFrom(_) | Provenance::Unknown => any_unknown = true,
            }
        }

        // One event begins one lineage, however many handles onto it the action
        // hands back. A transformation yields both a strain and its culture,
        // and those are one organism, not two.
        let mut event = None;
        for (position, result) in results.iter().enumerate() {
            let lineage = declared
                .and_then(|lineages| lineages.get(position))
                .copied()
                .unwrap_or_default();
            let provenance = match lineage {
                Lineage::Begins => {
                    let origin = *event.get_or_insert_with(|| {
                        let origin = Origin(self.next);
                        self.next += 1;
                        origin
                    });
                    // A collection of beginnings is a family whose members are
                    // independent of one another — the colonies of one pick.
                    if is_collection(&result.r#type) {
                        Provenance::EachFrom(origin)
                    } else {
                        Provenance::From(BTreeSet::from([origin]))
                    }
                }
                Lineage::Continues if any_unknown => Provenance::Unknown,
                // A result that continues nothing must start something: no
                // material flowed in, so two of these are as independent as two
                // separate assemblies or two separate batches of cells.
                Lineage::Continues if inherited.is_empty() => {
                    let origin = *event.get_or_insert_with(|| {
                        let origin = Origin(self.next);
                        self.next += 1;
                        origin
                    });
                    Provenance::From(BTreeSet::from([origin]))
                }
                Lineage::Continues => Provenance::From(inherited.clone()),
            };
            self.map.bindings.insert(result.name.clone(), provenance);
        }
    }
}

fn is_collection(r#type: &CheckedType) -> bool {
    matches!(r#type, CheckedType::List { .. })
}

fn mentions_material(r#type: &CheckedType) -> bool {
    match r#type {
        CheckedType::Named { name, arguments } => {
            name == "Material" || arguments.iter().any(mentions_material)
        }
        CheckedType::List { element } => mentions_material(element),
        CheckedType::Union { alternatives } => alternatives.iter().any(mentions_material),
        _ => false,
    }
}

/// Whether two materials are the same biological entity, which is what makes
/// measuring both a technical replicate rather than a biological one.
pub fn same_entity(left: &Provenance, right: &Provenance) -> bool {
    match (left.origins(), right.origins()) {
        (Some(left), Some(right)) => !left.is_empty() && left == right,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::compile_module;
    use crate::standard_library::StandardLibrary;

    fn lineage_of(source: &str, workflow: &str) -> LineageMap {
        let module = compile_module(source).expect("the module checks");
        let table = lineage_table(&StandardLibrary::bundled());
        let body = module
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                crate::checked::CheckedDeclaration::Workflow { name, body, .. }
                    if name == workflow =>
                {
                    Some(body)
                }
                _ => None,
            })
            .expect("the workflow is declared");
        analyze(body, &table)
    }

    const SETUP: &str = r#"use std.lab.plasmid
use std.bio.designs

buy chassis DH5alpha
buy antibiotic chloramphenicol

strain host:
  chassis = DH5alpha
  plasmids = []

plasmid p_reporter:
  sequence = dna("ACGT")

"#;

    /// Recovering and plating do not make a second organism, so everything
    /// downstream of one transformation is the same entity.
    #[test]
    fn splitting_one_culture_keeps_one_lineage() {
        let map = lineage_of(
            &format!(
                "{SETUP}{}",
                r#"workflow build(carried: Material<Plasmid>) -> (strain: Material<Strain>, plate: Material<Plate>):
  dependencies = [carried]
  cells <- provision DH5alpha
  strain, culture <- transform host from dependencies into cells
  culture <- recover culture for 1 h
  plate <- plate culture on chloramphenicol
  return strain, plate
"#
            ),
            "build",
        );
        let strain = map.get("strain").expect("strain is bound");
        let plate = map.get("plate").expect("plate is bound");
        assert!(
            same_entity(strain, plate),
            "a plate of a recovered culture is the same organism: {strain:?} vs {plate:?}"
        );
        assert_eq!(strain.independent_count(), Some(1));
    }

    /// A transformation establishes an organism, so its culture owes nothing to
    /// the lineage of the DNA that went in — which arrived from a caller and is
    /// itself unknown.
    #[test]
    fn transformation_begins_a_lineage() {
        let map = lineage_of(
            &format!(
                "{SETUP}{}",
                r#"workflow build(carried: Material<Plasmid>) -> (strain: Material<Strain>, plate: Material<Plate>):
  dependencies = [carried]
  cells <- provision DH5alpha
  strain, culture <- transform host from dependencies into cells
  plate <- plate culture on chloramphenicol
  return strain, plate
"#
            ),
            "build",
        );
        assert_eq!(
            map.get("strain")
                .expect("strain is bound")
                .independent_count(),
            Some(1),
            "one organism, even though the plasmid arrived from a caller"
        );
    }

    /// Each picked colony is an independent transformant, so a pick yields a
    /// family whose members are biological replicates of one another. How many
    /// there are is a runtime value, so the count is not statically known.
    #[test]
    fn picking_colonies_begins_a_family_of_lineages() {
        let map = lineage_of(
            &format!(
                "{SETUP}{}",
                r#"workflow build(carried: Material<Plasmid>) -> (strain: Material<Strain>, plate: Material<Plate>):
  dependencies = [carried]
  cells <- provision DH5alpha
  strain, culture <- transform host from dependencies into cells
  plate <- plate culture on chloramphenicol
  candidates <- pick 4 isolated colonies from plate
  screening <- screen candidates against p_reporter
  <- dispose screening.clones.highest_confidence
  return strain, plate
"#
            ),
            "build",
        );
        let candidates = map.get("candidates").expect("candidates is bound");
        assert!(
            matches!(candidates, Provenance::EachFrom(_)),
            "picked colonies are independent of one another: {candidates:?}"
        );
        assert_eq!(
            candidates.independent_count(),
            None,
            "how many colonies were picked is a runtime value"
        );
        assert_eq!(
            map.get("screening"),
            Some(&Provenance::Unknown),
            "what continues from a family spans as many entities as the family holds, which is a runtime value"
        );
    }

    /// A workflow parameter comes from a caller this analysis cannot see, so
    /// what continues from it is unknown rather than assumed independent.
    #[test]
    fn a_parameter_has_unknown_provenance() {
        let map = lineage_of(
            r#"use std.lab.plasmid

workflow measure(sample: Material<Plasmid>) -> Material<Plasmid>:
  evidence <- quantify sample
  return sample
"#,
            "measure",
        );
        assert_eq!(map.get("evidence"), Some(&Provenance::Unknown));
    }
}
