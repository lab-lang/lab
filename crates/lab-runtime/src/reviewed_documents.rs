//! Built-in reviewed-document loaders.
//!
//! Each loader validates one document contract and returns an adapter-defined typed payload. The
//! application chooses which exact `(adapter ID, format)` pairs to register; this module does not
//! contain a global adapter switch or implicit defaults.

use anyhow::{Context, Result, bail};
use hamilton_star::RawCommand;
use lab_runfmt::{
    OPENTRONS_PROTOCOL_DESIGNER_FORMAT, OPENTRONS_PYTHON_PROTOCOL_FORMAT, PLATE_READ_FORMAT,
    PlateReadDocument, SIMULATION_RUN_FORMAT, STAR_RUN_FORMAT, SimulationRunDocument,
    StarRunDocument, THERMOCYCLE_RUN_FORMAT, ThermocycleRunDocument,
};
use sbol3::Iri;

use crate::execution::{LoadedReviewedDocument, ReviewedDocumentLoadRequest};

/// Validated Hamilton STAR run data consumed by the live STAR executor.
#[derive(Debug)]
pub struct LoadedStarRun {
    pub document: StarRunDocument,
    pub commands: Vec<RawCommand>,
}

/// A validated reviewed file delegated to a vendor application rather than a live connector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedExternalFile {
    pub contents: Vec<u8>,
}

pub fn load_star_run(request: ReviewedDocumentLoadRequest<'_>) -> Result<LoadedReviewedDocument> {
    let document: StarRunDocument = parse_json_document(request.bytes, request.path)?;
    require_declared_format(&document.format, STAR_RUN_FORMAT, request.path)?;
    if !document.manual_after.is_empty() {
        bail!(
            "{} carries manual-after steps; facility execution requires explicit Manual plan nodes",
            request.path.display()
        );
    }
    let commands = document
        .steps
        .iter()
        .map(|step| {
            RawCommand::parse(&step.frame).with_context(|| {
                format!(
                    "{} carries an unreplayable STAR frame",
                    request.path.display()
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let title = document.title.clone();
    LoadedReviewedDocument::new(STAR_RUN_FORMAT, title, LoadedStarRun { document, commands })
}

pub fn load_thermocycle_run(
    request: ReviewedDocumentLoadRequest<'_>,
) -> Result<LoadedReviewedDocument> {
    let document: ThermocycleRunDocument = parse_json_document(request.bytes, request.path)?;
    require_declared_format(&document.format, THERMOCYCLE_RUN_FORMAT, request.path)?;
    document
        .run
        .validate(&lab_instruments::odtc_thermal_limits())
        .with_context(|| {
            format!(
                "{} is outside the Inheco ODTC run contract",
                request.path.display()
            )
        })?;
    let title = document.title.clone();
    LoadedReviewedDocument::new(THERMOCYCLE_RUN_FORMAT, title, document)
}

pub fn load_plate_read(request: ReviewedDocumentLoadRequest<'_>) -> Result<LoadedReviewedDocument> {
    let document: PlateReadDocument = parse_json_document(request.bytes, request.path)?;
    require_declared_format(&document.format, PLATE_READ_FORMAT, request.path)?;
    let title = document.title.clone();
    LoadedReviewedDocument::new(PLATE_READ_FORMAT, title, document)
}

pub fn load_simulation_run(
    request: ReviewedDocumentLoadRequest<'_>,
) -> Result<LoadedReviewedDocument> {
    let document: SimulationRunDocument = parse_json_document(request.bytes, request.path)?;
    require_declared_format(&document.format, SIMULATION_RUN_FORMAT, request.path)?;
    Iri::new(document.capability_kind.clone()).with_context(|| {
        format!(
            "{} declares an invalid capability-kind IRI",
            request.path.display()
        )
    })?;
    if document.capability_kind != request.expected_capability_kind {
        bail!(
            "{} simulates capability '{}', but its frozen requirement binds '{}'",
            request.path.display(),
            document.capability_kind,
            request.expected_capability_kind
        );
    }
    let title = document.title.clone();
    LoadedReviewedDocument::new(SIMULATION_RUN_FORMAT, title, document)
}

pub fn load_opentrons_python_protocol(
    request: ReviewedDocumentLoadRequest<'_>,
) -> Result<LoadedReviewedDocument> {
    let source = std::str::from_utf8(request.bytes).with_context(|| {
        format!(
            "{} is not a UTF-8 Opentrons Python protocol",
            request.path.display()
        )
    })?;
    for marker in [
        "from opentrons import protocol_api",
        "def run(protocol: protocol_api.ProtocolContext) -> None:",
        "# LAB:INVOCATION_PLAN",
    ] {
        if !source.contains(marker) {
            bail!(
                "{} is missing required Opentrons protocol marker {:?}",
                request.path.display(),
                marker
            );
        }
    }
    LoadedReviewedDocument::new(
        OPENTRONS_PYTHON_PROTOCOL_FORMAT,
        format!(
            "Opentrons OT-2 {} protocol",
            capability_name(request.expected_capability_kind)
        ),
        LoadedExternalFile {
            contents: request.bytes.to_vec(),
        },
    )
}

pub fn load_opentrons_protocol_designer(
    request: ReviewedDocumentLoadRequest<'_>,
) -> Result<LoadedReviewedDocument> {
    let protocol: serde_json::Value = parse_json_document(request.bytes, request.path)?;
    if protocol
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(8)
    {
        bail!(
            "{} is not an Opentrons Protocol Designer schema 8 document",
            request.path.display()
        );
    }
    if protocol
        .pointer("/robot/model")
        .and_then(serde_json::Value::as_str)
        != Some("OT-3 Standard")
    {
        bail!(
            "{} does not target the Opentrons Flex robot model",
            request.path.display()
        );
    }
    let commands = protocol
        .get("commands")
        .and_then(serde_json::Value::as_array)
        .with_context(|| {
            format!(
                "{} has no Protocol Designer command sequence",
                request.path.display()
            )
        })?;
    if commands.is_empty() {
        bail!(
            "{} has an empty Protocol Designer command sequence",
            request.path.display()
        );
    }
    LoadedReviewedDocument::new(
        OPENTRONS_PROTOCOL_DESIGNER_FORMAT,
        format!(
            "Opentrons Flex {} protocol",
            capability_name(request.expected_capability_kind)
        ),
        LoadedExternalFile {
            contents: request.bytes.to_vec(),
        },
    )
}

fn parse_json_document<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    path: &std::path::Path,
) -> Result<T> {
    serde_json::from_slice(bytes)
        .with_context(|| format!("{} is not a valid reviewed run document", path.display()))
}

fn require_declared_format(actual: &str, expected: &str, path: &std::path::Path) -> Result<()> {
    if actual != expected {
        bail!(
            "{} declares format '{}', expected '{}'",
            path.display(),
            actual,
            expected
        );
    }
    Ok(())
}

fn capability_name(capability_kind: &str) -> &str {
    capability_kind
        .rsplit(['#', '/'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(capability_kind)
}
