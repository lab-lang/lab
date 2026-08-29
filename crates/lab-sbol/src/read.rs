//! Reading Lab declarations out of an SBOL document.
//!
//! A design written in SBOL becomes the same checked declaration a design
//! written in Lab becomes, and it becomes one the same way: this module builds
//! declarations and hands them to the checker. Nothing here decides whether a
//! property belongs on a kind, whether a value has the right type, or whether a
//! schema is complete. Those rules have one implementation, and a design read
//! from a document is subject to it exactly as a design typed by hand is.
//!
//! Declarations are built rather than printed. Rendering Lab source and parsing
//! it back would work and would be a mistake: errors would point at text nobody
//! wrote, which is the cost the Python SDK paid for exactly this shortcut.
//!
//! Provenance is the one thing an imported design cannot carry. Whether a
//! laboratory builds a plasmid or orders it is a fact about that laboratory and
//! not about the design, per decision 0027, and a registry has no opinion. An
//! imported component is therefore catalogued: a registry listing something is
//! exactly the claim that you can obtain it. A document Lab wrote says
//! otherwise in its own namespace, and that is honoured where present.

use std::collections::{BTreeMap, BTreeSet};

use lab_language::ast::instance_word;
use lab_language::ast::{
    Argument, ArtifactDecl, ArtifactMember, Expr, Item, Module, Path, PropertyDecl, Provenance,
    UseDecl,
};
use lab_language::{CheckedModule, ModuleError, ModuleId, SemanticEnvironment, Span, Spanned};
use sbol3::{Component, Document, SbolIdentified, SbolObject, Term};

use crate::kind::{KindIndex, LAB_KIND, Resolution};

/// The ordering relation SBOL states between two features that abut.
const SBOL_MEETS: &str = "http://sbols.org/v3#meets";

/// Why a component could not become a declaration.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error("'{identity}' states no ontology terms, so nothing says what it is")]
    Ungrounded { identity: String },
    #[error("'{identity}' is a {candidates}, and nothing in the document says which")]
    AmbiguousKind {
        identity: String,
        candidates: String,
    },
    #[error("'{identity}' states terms no kind in scope stands for")]
    UnknownKind { identity: String },
    #[error("'{identity}' names the kind '{kind}', which is not in scope")]
    StatedKindUnknown { identity: String, kind: String },
    #[error("'{identity}' has no displayId, so it cannot be given a Lab name")]
    Unnamed { identity: String },
    #[error("'{identity}' carries {count} sequences, and a design states one")]
    SeveralSequences { identity: String, count: usize },
    #[error("'{identity}' refers to '{reference}', which the document does not contain")]
    Dangling { identity: String, reference: String },
    #[error("'{identity}' is built from '{feature}', which is not a sub-component of a component")]
    UnsupportedFeature { identity: String, feature: String },
    #[error("'{identity}' does not say what order its parts are joined in")]
    Unorderable { identity: String },
}

/// One component that could not be read, kept beside the ones that could.
///
/// A registry export is large and partly outside any one program's vocabulary,
/// so refusing the whole document because of one unrecognized component would
/// make the feature unusable. The caller decides which problems are fatal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub identity: String,
    pub reason: ReadError,
}

/// The declarations one SBOL document contributed, before checking.
#[derive(Clone, Debug, PartialEq)]
pub struct Read {
    pub module: Module,
    pub skipped: Vec<Skipped>,
}

/// Builds the declarations every component in `document` states.
///
/// The result has not been checked. [`read_module`] returns one that has, and
/// is what a caller normally wants.
pub fn read_designs(document: &Document, kinds: &KindIndex) -> Read {
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    for component in document.components() {
        match declaration_for(document, component, kinds) {
            Ok(declaration) => items.push(Item::Artifact(declaration)),
            Err(reason) => skipped.push(Skipped {
                identity: component.identity.to_string(),
                reason,
            }),
        }
    }
    // A registry hands its components back in whatever order its store keeps
    // them, and a build should not change because of that.
    items.sort_by(|left, right| item_name(left).cmp(item_name(right)));
    skipped.sort_by(|left, right| left.identity.cmp(&right.identity));
    Read {
        module: Module {
            doc: None,
            items,
            span: Span::new(0, 0),
        },
        skipped,
    }
}

/// Reads a document and checks what it read.
///
/// A document names the terms its components stand for and never says which Lab
/// package describes them, so the imports are derived rather than configured:
/// whichever kinds the document turned out to use, the modules declaring those
/// kinds are what it imports. A registry export needs no accompanying
/// configuration to be readable, and a document that grows a new kind of part
/// does not need one written for it either.
pub fn read_module(
    module_id: ModuleId,
    document: &Document,
    kinds: &KindIndex,
    environment: &SemanticEnvironment,
) -> (Result<CheckedModule, ModuleError>, Vec<Skipped>) {
    let read = read_designs(document, kinds);
    let mut items: Vec<Item> = imports_for(&read.module, kinds)
        .into_iter()
        .map(|module| {
            Item::Use(UseDecl {
                path: path(&module),
                span: Span::new(0, 0),
            })
        })
        .collect();
    items.extend(read.module.items);
    let module = Module {
        items,
        ..read.module
    };
    (
        lab_language::compile_ast_module(module_id, environment, &module),
        read.skipped,
    )
}

/// The modules declaring every kind this module's declarations were written
/// with, in a stable order.
fn imports_for(module: &Module, kinds: &KindIndex) -> BTreeSet<String> {
    module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Artifact(declaration) => Some(&declaration.kind.value),
            _ => None,
        })
        .filter_map(|word| kinds.module_for_word(word))
        .map(str::to_owned)
        .collect()
}

fn declaration_for(
    document: &Document,
    component: &Component,
    kinds: &KindIndex,
) -> Result<ArtifactDecl, ReadError> {
    let identity = component.identity.to_string();
    let Some(display_id) = component.display_id() else {
        return Err(ReadError::Unnamed { identity });
    };
    let kind = kind_of(component, kinds, &identity)?;

    // The registry's Component IRI is biological design identity. It is not a
    // supplier order number and remains the same whether a laboratory builds
    // or buys a realization of the design.
    let mut members = vec![property("sbol_identity", string(&identity))];
    if let Some(sequence) = sequence_expression(document, component, &identity)? {
        members.push(property("sequence", sequence));
    }
    if let Some(components) = components_expression(document, component, kinds, &identity)? {
        members.push(property("components", components));
    }

    Ok(ArtifactDecl {
        doc: documentation(component),
        provenance: Provenance::Buy,
        kind: name(&instance_word(&kind)),
        name: name(display_id),
        ascribed: None,
        members,
        span: Span::new(0, 0),
    })
}

fn kind_of(component: &Component, kinds: &KindIndex, identity: &str) -> Result<String, ReadError> {
    match stated_kind(component) {
        Some(stated) if kinds.terms(&stated).is_some() => Ok(stated),
        Some(stated) => Err(ReadError::StatedKindUnknown {
            identity: identity.to_owned(),
            kind: stated,
        }),
        None => infer_kind(component, kinds, identity),
    }
}

/// The kind a document Lab wrote states outright.
fn stated_kind(component: &Component) -> Option<String> {
    component.extensions().iter().find_map(|extension| {
        (extension.predicate.as_str() == LAB_KIND)
            .then(|| match &extension.object {
                Term::Literal(literal) => Some(literal.value().to_owned()),
                // A kind is a word, so a resource here is a document saying
                // something this reader has no interpretation for.
                _ => None,
            })
            .flatten()
    })
}

fn infer_kind(
    component: &Component,
    kinds: &KindIndex,
    identity: &str,
) -> Result<String, ReadError> {
    let terms: BTreeSet<String> = component
        .types
        .iter()
        .chain(component.roles.iter())
        .map(|iri| iri.as_str().to_owned())
        .collect();
    if terms.is_empty() {
        return Err(ReadError::Ungrounded {
            identity: identity.to_owned(),
        });
    }
    match kinds.resolve(&terms) {
        Resolution::Resolved(kind) => Ok(kind),
        Resolution::Ambiguous(candidates) => Err(ReadError::AmbiguousKind {
            identity: identity.to_owned(),
            candidates: candidates.join(" or a "),
        }),
        Resolution::Unresolved => Err(ReadError::UnknownKind {
            identity: identity.to_owned(),
        }),
    }
}

/// The component's sequence, written the way an author writes one.
///
/// SBOL lets a component carry several sequences, one per encoding, and Lab's
/// `sequence` is one value of one type. Reading the first of several would pick
/// silently between a nucleotide and a protein spelling of the same design, so
/// several is reported instead.
fn sequence_expression(
    document: &Document,
    component: &Component,
    identity: &str,
) -> Result<Option<Expr>, ReadError> {
    let [only] = component.sequences.as_slice() else {
        if component.sequences.len() > 1 {
            return Err(ReadError::SeveralSequences {
                identity: identity.to_owned(),
                count: component.sequences.len(),
            });
        }
        return Ok(None);
    };
    let Some(SbolObject::Sequence(sequence)) = document.resolve(only) else {
        return Err(ReadError::Dangling {
            identity: identity.to_owned(),
            reference: only.to_string(),
        });
    };
    let Some(elements) = sequence.elements.as_deref() else {
        return Ok(None);
    };
    Ok(Some(Expr::Call {
        callee: Box::new(Expr::Path(path("dna"))),
        arguments: vec![Argument {
            name: None,
            value: string(elements),
            span: Span::new(0, 0),
        }],
        span: Span::new(0, 0),
    }))
}

/// What the component is assembled from, in the order they are joined.
///
/// SBOL states order as `meets` constraints between features and Lab states it
/// as the order of a list, so the chain is walked once here and the coordinates
/// it implies are left to be recomputed rather than carried. That is
/// normalization, not loss: a linear chain and an ordered list hold the same
/// fact.
fn components_expression(
    document: &Document,
    component: &Component,
    kinds: &KindIndex,
    identity: &str,
) -> Result<Option<Expr>, ReadError> {
    if component.features.is_empty() {
        return Ok(None);
    }
    let ordered = order_features(document, component, kinds, identity)?;
    Ok(Some(Expr::List {
        elements: ordered
            .into_iter()
            .map(|part| Expr::Path(path(&part)))
            .collect(),
        span: Span::new(0, 0),
    }))
}

/// The names of a component's sub-components, ordered head to tail.
///
/// A single feature needs no chain. Beyond that the `meets` constraints must
/// form one unambiguous line, which is the same shape `compute_sequence`
/// requires, and anything branching or broken is reported rather than
/// linearized arbitrarily: the wrong order silently builds the wrong construct.
fn order_features(
    document: &Document,
    component: &Component,
    kinds: &KindIndex,
    identity: &str,
) -> Result<Vec<String>, ReadError> {
    let mut instance_of = BTreeMap::new();
    for feature in &component.features {
        let Some(SbolObject::SubComponent(sub)) = document.resolve(feature) else {
            return Err(ReadError::UnsupportedFeature {
                identity: identity.to_owned(),
                feature: feature.to_string(),
            });
        };
        let Some(target) = sub.instance_of.as_ref() else {
            return Err(ReadError::UnsupportedFeature {
                identity: identity.to_owned(),
                feature: feature.to_string(),
            });
        };
        let Some(SbolObject::Component(part)) = document.resolve(target) else {
            return Err(ReadError::Dangling {
                identity: identity.to_owned(),
                reference: target.to_string(),
            });
        };
        let Some(part_name) = part.display_id() else {
            return Err(ReadError::Unnamed {
                identity: target.to_string(),
            });
        };
        // A part this program has no kind for cannot be referred to, so the
        // composite naming it is refused rather than left to dangle.
        kind_of(part, kinds, &target.to_string())?;
        instance_of.insert(feature.to_string(), part_name.to_owned());
    }

    if instance_of.len() == 1 {
        return Ok(instance_of.into_values().collect());
    }

    let mut next = BTreeMap::new();
    let mut has_predecessor = BTreeSet::new();
    for constraint in document.constraints() {
        let (Some(subject), Some(object)) = (
            constraint.subject.as_ref(),
            constraint.constrained_object.as_ref(),
        ) else {
            continue;
        };
        let (subject, object) = (subject.to_string(), object.to_string());
        if constraint.restriction.as_ref().map(sbol3::Iri::as_str) != Some(SBOL_MEETS)
            || !instance_of.contains_key(&subject)
            || !instance_of.contains_key(&object)
        {
            continue;
        }
        if next.insert(subject, object.clone()).is_some() || !has_predecessor.insert(object) {
            return Err(ReadError::Unorderable {
                identity: identity.to_owned(),
            });
        }
    }

    let heads: Vec<&String> = instance_of
        .keys()
        .filter(|feature| !has_predecessor.contains(*feature))
        .collect();
    let [head] = heads.as_slice() else {
        return Err(ReadError::Unorderable {
            identity: identity.to_owned(),
        });
    };

    let mut ordered = Vec::with_capacity(instance_of.len());
    let mut cursor = Some((*head).clone());
    while let Some(feature) = cursor {
        let Some(part_name) = instance_of.get(&feature) else {
            return Err(ReadError::Unorderable {
                identity: identity.to_owned(),
            });
        };
        ordered.push(part_name.clone());
        if ordered.len() > instance_of.len() {
            return Err(ReadError::Unorderable {
                identity: identity.to_owned(),
            });
        }
        cursor = next.get(&feature).cloned();
    }
    if ordered.len() != instance_of.len() {
        return Err(ReadError::Unorderable {
            identity: identity.to_owned(),
        });
    }
    Ok(ordered)
}

/// What a reader has to say about a component in prose.
///
/// SBOL's `name` is a human-readable label and its `description` is prose, and
/// neither is a property any kind declares. They are documentation, and the
/// checker would reject them as properties, which is the point: those rules
/// have one home and this is not it.
fn documentation(component: &Component) -> Option<String> {
    match (component.name(), component.description()) {
        (Some(name), Some(description)) => Some(format!("{name}. {description}")),
        (Some(text), None) | (None, Some(text)) => Some(text.to_owned()),
        (None, None) => None,
    }
}

fn name(value: &str) -> Spanned<String> {
    Spanned::new(value.to_owned(), Span::new(0, 0))
}

fn path(value: &str) -> Path {
    Path {
        segments: value.split('.').map(name).collect(),
        span: Span::new(0, 0),
    }
}

fn string(value: &str) -> Expr {
    Expr::String {
        value: value.to_owned(),
        span: Span::new(0, 0),
    }
}

fn property(field: &str, value: Expr) -> ArtifactMember {
    ArtifactMember::Property(PropertyDecl {
        name: name(field),
        value,
        span: Span::new(0, 0),
    })
}

fn item_name(item: &Item) -> &str {
    match item {
        Item::Artifact(declaration) => &declaration.name.value,
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lab_language::{CheckedDeclaration, Grounding};
    use sbol3::RdfFormat;

    const NAMESPACE: &str = "https://synbiohub.org/public/igem";

    fn document(components: &[(&str, &[&str], Option<&str>)]) -> Document {
        let mut objects = Vec::new();
        for (display_id, terms, name) in components {
            let mut builder = sbol3::Component::builder(NAMESPACE, *display_id)
                .expect("a valid namespace and displayId");
            for term in *terms {
                builder = builder.add_type(sbol3::Iri::new(*term).expect("a valid term IRI"));
            }
            if let Some(name) = name {
                builder = builder.name(*name);
            }
            objects.push(sbol3::SbolObject::Component(
                builder.build().expect("a complete component"),
            ));
        }
        Document::from_objects(objects).expect("distinct identities")
    }

    fn kinds() -> KindIndex {
        KindIndex::new(&Grounding::bundled())
    }

    fn resource(iri: &str) -> sbol3::Resource {
        sbol3::Resource::Iri(sbol3::Iri::new(iri).expect("a valid IRI"))
    }

    fn part(display_id: &str, role: &str) -> SbolObject {
        SbolObject::Component(
            sbol3::Component::builder(NAMESPACE, display_id)
                .expect("valid")
                .add_type(sbol3::Iri::new(term("SBO:0000251")).expect("valid"))
                .add_component_role(sbol3::Iri::new(term(role)).expect("valid"))
                .build()
                .expect("complete"),
        )
    }

    /// A promoter, RBS, coding sequence, and terminator joined head to tail
    /// under one composite, with the parts deliberately declared in an order
    /// that is not the assembled order so the `meets` chain is what decides.
    fn transcription_unit_objects() -> Vec<SbolObject> {
        let parts = [
            ("p_j23101", "SO:0000167"),
            ("term_b0015", "SO:0000141"),
            ("cds_gfp", "SO:0000316"),
            ("rbs_b0034", "SO:0000139"),
        ];
        let mut objects: Vec<SbolObject> = parts
            .iter()
            .map(|(display_id, role)| part(display_id, role))
            .collect();

        let composite_iri = format!("{NAMESPACE}/tu");
        let sequence = sbol3::Sequence::builder(NAMESPACE, "tu_sequence")
            .expect("valid")
            .elements("TTGACAGCTAGCATGGGCTAA")
            .encoding(sbol3::constants::EDAM_IUPAC_DNA)
            .build()
            .expect("complete");

        let mut composite = sbol3::Component::builder(NAMESPACE, "tu")
            .expect("valid")
            .add_type(sbol3::Iri::new(term("SBO:0000251")).expect("valid"))
            .add_component_role(sbol3::Iri::new(term("SO:0000804")).expect("valid"))
            .add_sequence(sequence.identity.clone())
            .extension(
                sbol3::Iri::new(LAB_KIND).expect("valid"),
                Term::Literal(sbol3::Literal::simple("Plasmid")),
            );

        // Assembled order, which is neither declaration order nor the order the
        // features sort in: feature `f0` is the *last* part. Nothing but the
        // `meets` chain can produce the right answer, so a reader that fell
        // back on any incidental ordering would fail this fixture.
        let assembled = ["p_j23101", "rbs_b0034", "cds_gfp", "term_b0015"];
        let last = assembled.len() - 1;
        let mut features = Vec::new();
        for (index, target) in assembled.iter().enumerate() {
            let feature = sbol3::SubComponent::builder(
                &resource(&composite_iri),
                format!("f{}", last - index),
            )
            .expect("valid")
            .instance_of(resource(&format!("{NAMESPACE}/{target}")))
            .build()
            .expect("complete");
            composite = composite.add_feature(feature.identity.clone());
            features.push(feature);
        }

        let composite = composite.build().expect("complete");
        let mut constraints = Vec::new();
        for index in 0..features.len() - 1 {
            constraints.push(SbolObject::Constraint(
                sbol3::Constraint::builder(&resource(&composite_iri), format!("c{index}"))
                    .expect("valid")
                    .subject(features[index].identity.clone())
                    .constrained_object(features[index + 1].identity.clone())
                    .restriction(sbol3::Iri::from_static(SBOL_MEETS))
                    .build()
                    .expect("complete"),
            ));
        }

        objects.push(SbolObject::Sequence(sequence));
        objects.push(SbolObject::Component(composite));
        objects.extend(features.into_iter().map(SbolObject::SubComponent));
        objects.extend(constraints);
        objects
    }

    fn transcription_unit() -> Document {
        Document::from_objects(transcription_unit_objects()).expect("distinct identities")
    }

    fn term(compact: &str) -> String {
        format!("https://identifiers.org/{compact}")
    }

    fn environment() -> SemanticEnvironment {
        SemanticEnvironment::default()
    }

    fn check(document: &Document) -> (CheckedModule, Vec<Skipped>) {
        let (checked, skipped) = read_module(
            ModuleId::new("registry"),
            document,
            &kinds(),
            &environment(),
        );
        (
            checked.unwrap_or_else(|error| panic!("the read module must check: {error}")),
            skipped,
        )
    }

    fn catalogued<'a>(module: &'a CheckedModule, name: &str) -> &'a CheckedDeclaration {
        module
            .declarations
            .iter()
            .find(|declaration| {
                matches!(declaration, CheckedDeclaration::Catalog { name: found, .. } if found == name)
            })
            .unwrap_or_else(|| panic!("'{name}' is declared"))
    }

    /// A registry export becomes catalogued declarations that the checker
    /// accepted, typed by what the document says each part is.
    #[test]
    fn a_registry_export_becomes_checked_catalogued_declarations() {
        let document = document(&[
            (
                "BBa_J23101",
                &[&term("SBO:0000251"), &term("SO:0000167")],
                Some("constitutive promoter"),
            ),
            ("chlor", &[&term("SBO:0000247")], None),
        ]);
        let (module, skipped) = check(&document);
        assert!(skipped.is_empty(), "{skipped:?}");

        let CheckedDeclaration::Catalog {
            r#type,
            sbol_identity,
            doc,
            ..
        } = catalogued(&module, "BBa_J23101")
        else {
            panic!("catalogued");
        };
        assert_eq!(r#type.to_string(), "Promoter");
        assert_eq!(
            sbol_identity.as_deref(),
            Some("https://synbiohub.org/public/igem/BBa_J23101")
        );
        assert_eq!(doc.as_deref(), Some("constitutive promoter"));

        // The module publishes what it read, so a later `use` resolves it.
        assert!(module.interface.exports.contains_key("BBa_J23101"));
        assert!(module.interface.exports.contains_key("chlor"));
    }

    /// The ordinary case for a registry: a part that publishes its sequence.
    /// A kind that could not state one would reject almost every real part.
    #[test]
    fn an_atomic_part_carries_the_sequence_its_registry_publishes() {
        let sequence = sbol3::Sequence::builder(NAMESPACE, "j23101_sequence")
            .expect("valid")
            .elements("TTGACAGCTAGCTCAGTCCTAGGTATAGTGCTAGC")
            .encoding(sbol3::constants::EDAM_IUPAC_DNA)
            .build()
            .expect("complete");
        let component = sbol3::Component::builder(NAMESPACE, "BBa_J23101")
            .expect("valid")
            .add_type(sbol3::Iri::new(term("SBO:0000251")).expect("valid"))
            .add_component_role(sbol3::Iri::new(term("SO:0000167")).expect("valid"))
            .add_sequence(sequence.identity.clone())
            .build()
            .expect("complete");
        let document = Document::from_objects(vec![
            SbolObject::Sequence(sequence),
            SbolObject::Component(component),
        ])
        .expect("distinct identities");

        let (module, skipped) = check(&document);
        assert!(skipped.is_empty(), "{skipped:?}");
        let CheckedDeclaration::Catalog { properties, .. } = catalogued(&module, "BBa_J23101")
        else {
            panic!("catalogued");
        };
        let names: Vec<&str> = properties.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["sequence"]);
    }

    /// The point of building declarations rather than checked IR: the checker
    /// is what decides a design is well formed, so a reader cannot produce
    /// something an author could not have written.
    #[test]
    fn the_checker_rejects_a_property_no_schema_declares() {
        let document = document(&[("chlor", &[&term("SBO:0000247")], None)]);
        let mut read = read_designs(&document, &kinds());
        let Item::Artifact(declaration) = &mut read.module.items[0] else {
            panic!("an artifact");
        };
        declaration
            .members
            .push(property("sequence", string("ACGT")));
        read.module.items.insert(
            0,
            Item::Use(UseDecl {
                path: path("std.bio.designs"),
                span: Span::new(0, 0),
            }),
        );

        let error = lab_language::compile_ast_module(
            ModuleId::new("registry"),
            &environment(),
            &read.module,
        )
        .expect_err("an antibiotic declares no sequence");
        assert!(format!("{error}").contains("sequence"), "{error}");
    }

    /// A composite: four parts joined head to tail plus the sequence the
    /// assembly produces, checked against the plasmid schema.
    #[test]
    fn a_composite_becomes_ordered_components_and_a_sequence() {
        let (module, skipped) = check(&transcription_unit());
        assert!(skipped.is_empty(), "{skipped:?}");

        let CheckedDeclaration::Catalog { properties, .. } = catalogued(&module, "tu") else {
            panic!("catalogued");
        };
        let names: Vec<&str> = properties.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["sequence", "components"]);

        let components = properties
            .iter()
            .find(|property| property.name == "components")
            .expect("the composite states what it is built from");
        let lab_language::CheckedExpression::List { elements } = &components.value.value else {
            panic!("components are a list");
        };
        let parts: Vec<&str> = elements
            .iter()
            .map(|element| match &element.value {
                lab_language::CheckedExpression::Reference { path, .. } => path[0].as_str(),
                other => panic!("a component is a reference, found {other:?}"),
            })
            .collect();
        // Document order is deliberately not this order: the `meets` chain is.
        assert_eq!(
            parts,
            vec!["p_j23101", "rbs_b0034", "cds_gfp", "term_b0015"]
        );
    }

    /// A chain that closes into a ring says no part is first, and there is no
    /// honest way to choose one. Reporting beats linearizing arbitrarily,
    /// because the wrong order silently builds the wrong construct.
    #[test]
    fn a_cyclic_chain_is_reported_rather_than_linearized() {
        let mut objects = transcription_unit_objects();
        objects.push(SbolObject::Constraint(
            sbol3::Constraint::builder(&resource(&format!("{NAMESPACE}/tu")), "closing")
                .expect("valid")
                .subject(resource(&format!("{NAMESPACE}/tu/f0")))
                .constrained_object(resource(&format!("{NAMESPACE}/tu/f3")))
                .restriction(sbol3::Iri::from_static(SBOL_MEETS))
                .build()
                .expect("complete"),
        ));
        let document = Document::from_objects(objects).expect("distinct identities");

        let (_, skipped) = check(&document);
        let refused = skipped
            .iter()
            .find(|skipped| skipped.identity.ends_with("/tu"))
            .expect("the composite is refused");
        assert!(
            matches!(refused.reason, ReadError::Unorderable { .. }),
            "{:?}",
            refused.reason
        );
    }

    /// One unreadable component does not cost the rest of the document.
    #[test]
    fn an_unreadable_component_is_skipped_rather_than_failing_the_document() {
        let document = document(&[
            ("readable", &[&term("SBO:0000247")], None),
            ("mystery", &[&term("SO:0000694")], None),
        ]);
        let (module, skipped) = check(&document);
        assert_eq!(module.declarations.len(), 1);
        assert_eq!(skipped.len(), 1);
        assert!(matches!(skipped[0].reason, ReadError::UnknownKind { .. }));
    }

    /// Terms cannot separate a backbone from a plasmid, and the reader says so
    /// with both candidates named rather than choosing one.
    #[test]
    fn an_ambiguous_component_names_its_candidates() {
        let document = document(&[("pSB1C3", &[&term("SBO:0000251"), &term("SO:0000804")], None)]);
        let (_, skipped) = check(&document);
        let message = skipped[0].reason.to_string();
        assert!(message.contains("Backbone or a Plasmid"), "{message}");
    }

    /// A document Lab wrote states its own kind, so reading one back recovers
    /// what the author meant instead of re-deriving it from terms that cannot
    /// carry the distinction.
    #[test]
    fn a_stated_lab_kind_settles_what_terms_cannot() {
        let component = sbol3::Component::builder(NAMESPACE, "pSB1C3")
            .expect("valid")
            .add_type(sbol3::Iri::new(term("SBO:0000251")).expect("valid"))
            .add_type(sbol3::Iri::new(term("SO:0000804")).expect("valid"))
            .extension(
                sbol3::Iri::new(LAB_KIND).expect("valid"),
                Term::Literal(sbol3::Literal::simple("Backbone")),
            )
            .build()
            .expect("complete");
        let document =
            Document::from_objects(vec![SbolObject::Component(component)]).expect("one object");

        let (module, skipped) = check(&document);
        assert!(skipped.is_empty(), "{skipped:?}");
        let CheckedDeclaration::Catalog { r#type, .. } = catalogued(&module, "pSB1C3") else {
            panic!("catalogued");
        };
        assert_eq!(r#type.to_string(), "Backbone");
    }

    /// Reading real SBOL, not just objects built in memory.
    #[test]
    fn reads_a_serialized_document() {
        let turtle = transcription_unit()
            .write(RdfFormat::Turtle)
            .expect("the document serializes");
        let parsed = Document::read(&turtle, RdfFormat::Turtle).expect("it parses back");
        let (module, skipped) = check(&parsed);
        assert!(skipped.is_empty(), "{skipped:?}");
        assert!(module.interface.exports.contains_key("tu"));
    }
}
