//! LabOP export checked against an independent SBOL 3.1.0 implementation.
//!
//! The emitter and the checker share no code: `sbol3` parses the document and
//! applies the machine-checkable rules of the specification, so a convention
//! this backend gets wrong is caught by something that does not know how the
//! document was produced. The activity-level structure the SBOL rules do not
//! cover is asserted directly over the parsed statements.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

const SBOL: &str = "http://sbols.org/v3#";
const UML: &str = "http://bioprotocols.org/uml#";
const LABOP: &str = "http://bioprotocols.org/labop#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn emit(fixture_name: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_labc"))
        .args([fixture(fixture_name).to_str().unwrap(), "--emit", "labop"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "labop emission failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// One parsed statement. The document is N-Triples, so a line-oriented reading
/// is exact rather than approximate.
struct Statement {
    subject: String,
    predicate: String,
    object: String,
    is_iri: bool,
}

fn parse(document: &str) -> Vec<Statement> {
    document
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let rest = line
                .strip_suffix(" .")
                .unwrap_or_else(|| panic!("statement is unterminated: {line}"));
            let (subject, rest) = rest.split_once("> ").expect("statement has a subject");
            let (predicate, object) = rest.split_once("> ").expect("statement has a predicate");
            let is_iri = object.starts_with('<');
            Statement {
                subject: subject.trim_start_matches('<').to_owned(),
                predicate: predicate.trim_start_matches('<').to_owned(),
                object: object
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_owned(),
                is_iri,
            }
        })
        .collect()
}

fn objects<'a>(statements: &'a [Statement], predicate: &str) -> Vec<(&'a str, &'a str)> {
    statements
        .iter()
        .filter(|statement| statement.predicate == predicate)
        .map(|statement| (statement.subject.as_str(), statement.object.as_str()))
        .collect()
}

fn subjects_of_type<'a>(statements: &'a [Statement], class: &str) -> BTreeSet<&'a str> {
    statements
        .iter()
        .filter(|statement| statement.predicate == RDF_TYPE && statement.object == class)
        .map(|statement| statement.subject.as_str())
        .collect()
}

#[test]
fn the_document_satisfies_the_sbol3_specification() {
    let document = emit("reporter-library.lab");
    let parsed = sbol3::Document::read(&document, sbol3::RdfFormat::NTriples)
        .expect("emitted document is valid SBOL 3 RDF");
    let report = parsed.validate();
    assert!(
        !report.has_errors(),
        "sbol3 reported errors: {:#?}",
        report.errors().take(5).collect::<Vec<_>>()
    );
}

#[test]
fn the_document_is_canonically_sorted_and_free_of_duplicates() {
    let document = emit("reporter-library.lab");
    let lines: Vec<&str> = document.lines().filter(|l| !l.is_empty()).collect();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted, "document is not in canonical sorted order");
    let unique: BTreeSet<&&str> = lines.iter().collect();
    assert_eq!(unique.len(), lines.len(), "document repeats a statement");
}

/// A `displayId` that disagrees with the IRI it names is the failure mode that
/// makes a document parse and resolve to nothing.
#[test]
fn every_display_id_matches_its_iri_segment() {
    let document = emit("reporter-library.lab");
    let statements = parse(&document);
    let ids = objects(&statements, &format!("{SBOL}displayId"));
    assert!(!ids.is_empty());
    for (subject, display_id) in ids {
        let expected = display_id.trim_matches('"');
        let actual = subject.rsplit('/').next().expect("IRI has a segment");
        assert_eq!(actual, expected, "displayId disagrees with IRI {subject}");
    }
}

/// Every reference into a namespace this backend writes must resolve to an
/// object the same document declares.
#[test]
fn the_document_has_no_dangling_references() {
    let document = emit("reporter-library.lab");
    let statements = parse(&document);
    let declared: BTreeSet<&str> = statements
        .iter()
        .map(|statement| statement.subject.as_str())
        .collect();
    let namespace_predicate = format!("{SBOL}hasNamespace");
    for statement in &statements {
        if !statement.is_iri || statement.predicate == namespace_predicate {
            continue;
        }
        let owned = statement.object.starts_with("https://lab-lang.org/")
            || statement
                .object
                .starts_with("https://bioprotocols.org/labop/primitives/");
        if owned {
            assert!(
                declared.contains(statement.object.as_str()),
                "reference to undeclared {}",
                statement.object
            );
        }
    }
}

/// A pin whose name is not a parameter of the behavior its action calls is the
/// error LabOP's own type checking never performs.
#[test]
fn every_pin_names_a_parameter_of_its_behavior() {
    let document = emit("reporter-library.lab");
    let statements = parse(&document);

    let names: BTreeMap<&str, &str> = objects(&statements, &format!("{SBOL}name"))
        .into_iter()
        .map(|(subject, name)| (subject, name.trim_matches('"')))
        .collect();
    let property_value: BTreeMap<&str, &str> = objects(&statements, &format!("{UML}propertyValue"))
        .into_iter()
        .collect();

    let mut parameters: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (behavior, ordered) in objects(&statements, &format!("{UML}ownedParameter")) {
        let parameter = property_value
            .get(ordered)
            .unwrap_or_else(|| panic!("{ordered} carries no parameter"));
        let name = names
            .get(parameter)
            .unwrap_or_else(|| panic!("{parameter} has no name"));
        parameters.entry(behavior).or_default().insert(name);
    }

    let behaviors: BTreeMap<&str, &str> = objects(&statements, &format!("{UML}behavior"))
        .into_iter()
        .collect();
    let actions = subjects_of_type(&statements, &format!("{UML}CallBehaviorAction"));
    assert!(!actions.is_empty(), "document declares no actions");

    let mut checked = 0;
    for predicate in [format!("{UML}input"), format!("{UML}output")] {
        for (action, pin) in objects(&statements, &predicate) {
            if !actions.contains(action) {
                continue;
            }
            let behavior = behaviors
                .get(action)
                .unwrap_or_else(|| panic!("{action} names no behavior"));
            let declared = parameters
                .get(behavior)
                .unwrap_or_else(|| panic!("{behavior} declares no parameters"));
            let name = names
                .get(pin)
                .unwrap_or_else(|| panic!("pin {pin} has no name"));
            assert!(
                declared.contains(name),
                "pin '{name}' on {action} is not a parameter of {behavior}"
            );
            checked += 1;
        }
    }
    assert!(
        checked > 100,
        "expected a substantial protocol, saw {checked} pins"
    );
}

/// UML reads several object flows leaving one pin as nondeterministic, so a
/// value with more than one consumer has to fan out through a `ForkNode`.
#[test]
fn values_with_several_consumers_fan_out_through_a_fork() {
    let document = emit("reporter-library.lab");
    let statements = parse(&document);

    let flows = subjects_of_type(&statements, &format!("{UML}ObjectFlow"));
    let forks = subjects_of_type(&statements, &format!("{UML}ForkNode"));
    let actions = subjects_of_type(&statements, &format!("{UML}CallBehaviorAction"));
    assert!(!forks.is_empty(), "no fork was emitted");

    let sources: BTreeMap<&str, &str> = objects(&statements, &format!("{UML}source"))
        .into_iter()
        .filter(|(edge, _)| flows.contains(edge))
        .collect();

    let mut outgoing: BTreeMap<&str, usize> = BTreeMap::new();
    for source in sources.values() {
        *outgoing.entry(source).or_default() += 1;
    }
    for (node, count) in outgoing {
        if count > 1 {
            assert!(
                forks.contains(node) || actions.contains(node),
                "{node} has {count} outgoing object flows but is not a fork"
            );
        }
    }
}

/// Control flow must run from the initial node through every action to the
/// final node, since that ordering is all a LabOP activity says about sequence.
#[test]
fn every_protocol_is_connected_from_initial_to_final() {
    let document = emit("reporter-library.lab");
    let statements = parse(&document);

    let control = subjects_of_type(&statements, &format!("{UML}ControlFlow"));
    let sources: BTreeMap<&str, &str> = objects(&statements, &format!("{UML}source"))
        .into_iter()
        .filter(|(edge, _)| control.contains(edge))
        .collect();
    let targets: BTreeMap<&str, &str> = objects(&statements, &format!("{UML}target"))
        .into_iter()
        .filter(|(edge, _)| control.contains(edge))
        .collect();

    let mut successors: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (edge, source) in &sources {
        if let Some(target) = targets.get(edge) {
            successors.entry(source).or_default().push(target);
        }
    }

    let initials = subjects_of_type(&statements, &format!("{UML}InitialNode"));
    let finals = subjects_of_type(&statements, &format!("{UML}FinalNode"));
    let protocols = subjects_of_type(&statements, &format!("{LABOP}Protocol"));
    assert_eq!(protocols.len(), 4, "fixture builds four artifacts");

    for protocol in protocols {
        let nodes: BTreeSet<&str> = objects(&statements, &format!("{UML}node"))
            .into_iter()
            .filter(|(subject, _)| *subject == protocol)
            .map(|(_, node)| node)
            .collect();
        let initial = nodes
            .iter()
            .find(|node| initials.contains(*node))
            .unwrap_or_else(|| panic!("{protocol} has no initial node"));
        let terminal = nodes
            .iter()
            .find(|node| finals.contains(*node))
            .unwrap_or_else(|| panic!("{protocol} has no final node"));

        let mut reached = BTreeSet::new();
        let mut frontier = vec![*initial];
        while let Some(node) = frontier.pop() {
            for next in successors.get(node).into_iter().flatten() {
                if reached.insert(*next) {
                    frontier.push(next);
                }
            }
        }
        assert!(
            reached.contains(terminal),
            "{protocol} never reaches its final node"
        );
        // Forks carry data rather than control, so they are sequenced by the
        // actions they feed rather than by a control edge of their own.
        let forks = subjects_of_type(&statements, &format!("{UML}ForkNode"));
        for node in &nodes {
            if node == initial || forks.contains(node) {
                continue;
            }
            assert!(
                reached.contains(node),
                "{node} is unreachable in {protocol}"
            );
        }
    }
}

/// The projection is lossy, and the losses are the point: a reviewer comparing
/// the export against the Lab source has to be told what is missing.
#[test]
fn the_bundle_records_what_the_projection_dropped() {
    let output_dir = std::env::temp_dir().join(format!("lab-labop-test-{}", std::process::id()));
    if output_dir.exists() {
        std::fs::remove_dir_all(&output_dir).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_labc"))
        .args([
            fixture("reporter-library.lab").to_str().unwrap(),
            "--emit",
            "labop-bundle",
            "--output-dir",
            output_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "labop bundle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(output_dir.join("labop/protocol.nt").is_file());
    let omissions = std::fs::read_to_string(output_dir.join("labop/omissions.md")).unwrap();
    for expected in [
        "no loop construct",
        "material linearity is not represented",
        "LabOP records no lineage",
        "transformation replicates are not represented",
    ] {
        assert!(
            omissions.contains(expected),
            "omissions report does not mention '{expected}':\n{omissions}"
        );
    }
    std::fs::remove_dir_all(&output_dir).unwrap();
}
