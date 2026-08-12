//! LabOP emission: a Lab build written as an SBOL3 RDF interchange document.
//!
//! LabOP is an export target rather than a foundation. Information flows one
//! way, from verified Protocol LAIR into a weaker representation, so this
//! backend never re-derives anything: it restates operations LabOP has a
//! vocabulary for and reports what it had to drop.
//!
//! Nothing here depends on the LabOP Python distribution. A LabOP document is
//! SBOL3 RDF serialized as canonically sorted N-Triples, and the conventions a
//! reader relies on are the SBOL3 identity rules, which [`sbol`] implements.
//! The published primitive libraries are restated in [`library`] rather than
//! vendored, so the emitted document carries the behavior definitions its
//! actions reference.
//!
//! Where LabOP's libraries name no counterpart for a Lab operation, this
//! backend defines a primitive in a Lab namespace. `labop:Primitive` is a
//! `sbol:TopLevel`, so a document may carry its own behaviors; a consumer that
//! does not know them will read a well-formed activity whose steps it cannot
//! interpret, which is the same position it is in for the eleven of LabOP's
//! own fifty-two primitives that have executable definitions.

mod graph;
mod library;
mod lowering;
mod sbol;
mod triples;
mod vocabulary;

use std::collections::BTreeSet;

use thiserror::Error;

use crate::backend::TargetConstraintError;
use crate::backend::descriptor::{BackendDescriptor, BackendTarget};
use crate::backend::trace::analyze_protocol;
use crate::backend::traits::Backend;
use crate::{ArtifactBundle, ArtifactError, ProtocolLairProgram};

pub use lowering::Omission;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LabopCompileError {
    #[error(transparent)]
    Constraint(Box<TargetConstraintError>),
    #[error("invalid target-selected Protocol LAIR: {0}")]
    InvalidProtocol(String),
}

impl From<crate::backend::error::PlanningError> for LabopCompileError {
    fn from(error: crate::backend::error::PlanningError) -> Self {
        use crate::backend::error::PlanningError;
        match error {
            PlanningError::Constraint(constraint) => Self::Constraint(constraint),
            PlanningError::InvalidProtocol(message) => Self::InvalidProtocol(message),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LabopEmissionError {
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
}

/// A LabOP document and the record of what the source protocol stated that the
/// document cannot.
#[derive(Clone, Debug)]
pub struct LabopProgram {
    document: String,
    omissions: Vec<Omission>,
    protocols: Vec<String>,
    statements: usize,
}

impl LabopProgram {
    /// The document, as canonically sorted N-Triples.
    pub fn document(&self) -> &str {
        &self.document
    }

    /// What the projection dropped, in the order the artifacts were lowered.
    pub fn omissions(&self) -> &[Omission] {
        &self.omissions
    }

    /// Display identifiers of the protocols the document defines.
    pub fn protocols(&self) -> &[String] {
        &self.protocols
    }

    pub fn statement_count(&self) -> usize {
        self.statements
    }

    /// A companion note listing what the document does not carry. A reviewer
    /// comparing the LabOP export against the Lab source needs the losses
    /// stated somewhere, and the document itself has no place to put them.
    pub fn omissions_report(&self) -> String {
        use std::fmt::Write as _;

        let mut report = String::from("# LabOP export omissions\n\n");
        if self.omissions.is_empty() {
            report.push_str("This build projects into LabOP without loss.\n");
            return report;
        }
        report.push_str(
            "The emitted document states everything below only in prose, or not at all.\n\n",
        );
        let artifacts: BTreeSet<&str> = self
            .omissions
            .iter()
            .map(|omission| omission.artifact.as_str())
            .collect();
        for artifact in artifacts {
            let _ = writeln!(report, "## {artifact}\n");
            for omission in self
                .omissions
                .iter()
                .filter(|omission| omission.artifact == artifact)
            {
                let _ = writeln!(report, "- {}", omission.detail);
            }
            report.push('\n');
        }
        report
    }
}

/// Emits a Lab build as a LabOP interchange document.
#[derive(Clone, Debug, Default)]
pub struct LabopBackend {
    namespace: Option<String>,
}

impl LabopBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Overrides the namespace the emitted resources are published under.
    pub fn with_namespace(namespace: impl Into<String>) -> Self {
        Self {
            namespace: Some(namespace.into()),
        }
    }
}

impl Backend<ProtocolLairProgram> for LabopBackend {
    type Program = LabopProgram;
    type Error = LabopCompileError;

    fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor {
            id: "labop".into(),
            display_name: "Laboratory Open Protocol language".into(),
            manufacturer: None,
            targets: vec![BackendTarget {
                id: "labop.document".into(),
                display_name: "LabOP interchange document".into(),
                capabilities: BTreeSet::from([
                    "interchange".to_owned(),
                    "rdf".to_owned(),
                    "sbol3".to_owned(),
                ]),
            }],
        }
    }

    fn compile(&self, input: &ProtocolLairProgram) -> Result<Self::Program, Self::Error> {
        let traces = analyze_protocol(input, None)?;
        let namespace = self
            .namespace
            .clone()
            .unwrap_or_else(|| vocabulary::LAB_NAMESPACE.to_owned());
        let mut document = graph::Document::new(namespace);
        let lowered = lowering::lower(&mut document, &traces, input.context());
        Ok(LabopProgram {
            document: document.render(),
            omissions: lowered.omissions,
            protocols: lowered.protocols,
            statements: document.statement_count(),
        })
    }
}

/// Writes the document and its companion omissions report.
pub fn emit_program(program: &LabopProgram) -> Result<ArtifactBundle, LabopEmissionError> {
    let mut bundle = ArtifactBundle::new();
    bundle.insert_text(
        "labop/protocol.nt",
        "application/n-triples",
        program.document(),
    )?;
    bundle.insert_text(
        "labop/omissions.md",
        "text/markdown",
        program.omissions_report(),
    )?;
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_no_omissions_as_a_complete_projection() {
        let program = LabopProgram {
            document: String::new(),
            omissions: Vec::new(),
            protocols: Vec::new(),
            statements: 0,
        };
        assert!(program.omissions_report().contains("without loss"));
    }

    #[test]
    fn groups_omissions_by_artifact() {
        let program = LabopProgram {
            document: String::new(),
            omissions: vec![
                Omission {
                    artifact: "pTest".into(),
                    detail: "first".into(),
                },
                Omission {
                    artifact: "pTest".into(),
                    detail: "second".into(),
                },
            ],
            protocols: Vec::new(),
            statements: 0,
        };
        let report = program.omissions_report();
        assert_eq!(report.matches("## pTest").count(), 1);
        assert!(report.contains("- first"));
        assert!(report.contains("- second"));
    }
}
