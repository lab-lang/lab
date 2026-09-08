//! Post-run SBOLInventory result documents.
//!
//! A completed run produces a new graph. The reviewed source graph remains byte-for-byte
//! untouched; the result adds standard SBOL/PROV run records, exact Asset and MaterialLot
//! Usages, optional output MaterialLots and lineage, and hashed evidence Attachments.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lab_runfmt::{EXECUTION_PLAN_FILE, ExecutionMaterialOutput, ExecutionPlanAction};
use sbol_inventory::InventoryDocument;
use sbol_inventory::vocabulary::{
    DERIVED_FROM_MATERIAL, FACILITY_PROPERTY, IS_ACTIVE, LOCATED_IN, MATERIAL_KIND, POSITION,
    RUN_ASSET, RUN_INPUT_MATERIAL, XSD_BOOLEAN, XSD_STRING,
};
use sbol3::{
    Activity, Agent, Association, Attachment, ExperimentalData, HashAlgorithm, Implementation, Iri,
    Literal, Namespace, Plan, RdfFormat, RdfGraph, Resource, Term, ToRdf, Triple, Usage,
};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::execution::LoadedExecutionPlan;
use crate::ledger::LEDGER_FILE;
use crate::mode::ExecutionMode;

pub const INVENTORY_RESULT_FILE: &str = "inventory-after.ttl";
pub const SIMULATION_INVENTORY_RESULT_FILE: &str = "inventory-simulation.ttl";

pub const fn inventory_result_file(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Simulation => SIMULATION_INVENTORY_RESULT_FILE,
        ExecutionMode::Live => INVENTORY_RESULT_FILE,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryResult {
    pub path: PathBuf,
    pub activity: String,
    pub evidence: String,
    pub output_materials: Vec<String>,
}

/// Writes a new inventory result beside the reviewed plan after successful completion.
pub fn write_inventory_result(
    loaded: &LoadedExecutionPlan,
    mode: ExecutionMode,
    started_at_unix_seconds: u64,
    ended_at_unix_seconds: u64,
) -> Result<InventoryResult> {
    if mode == ExecutionMode::Simulation && !loaded.plan.outputs.is_empty() {
        bail!("simulation cannot generate physical output MaterialLots");
    }
    let output_path = loaded.directory.join(inventory_result_file(mode));
    if output_path == loaded.inventory.source_path() {
        bail!("refusing to overwrite the reviewed inventory source");
    }
    if output_path.exists() {
        bail!(
            "{} already exists; preserving prior run provenance rather than overwriting it",
            output_path.display()
        );
    }

    let started_at = timestamp(started_at_unix_seconds)?;
    let ended_at = timestamp(ended_at_unix_seconds)?;
    if ended_at_unix_seconds < started_at_unix_seconds {
        bail!("run completion time precedes its start time");
    }
    let run_namespace = Namespace::new(format!(
        "https://lab-lang.org/runs/{}/{}/{}",
        mode.as_str(),
        loaded.plan_sha256,
        started_at_unix_seconds
    ))?;
    let run_resource = iri_resource(format!("{}/run", run_namespace.as_str()))?;

    let plan_bytes = read_evidence_file(&loaded.directory, EXECUTION_PLAN_FILE)?;
    let ledger_bytes = read_evidence_file(&loaded.directory, LEDGER_FILE)?;
    let mut evidence_files = vec![EvidenceFile {
        display_id: "reviewed_plan_attachment".to_owned(),
        name: EXECUTION_PLAN_FILE.to_owned(),
        media_type: "https://www.iana.org/assignments/media-types/application/json".to_owned(),
        bytes: plan_bytes,
    }];
    evidence_files.push(EvidenceFile {
        display_id: "run_ledger_attachment".to_owned(),
        name: LEDGER_FILE.to_owned(),
        media_type: "https://www.iana.org/assignments/media-types/application/jsonl".to_owned(),
        bytes: ledger_bytes,
    });

    let mut child_paths = BTreeSet::new();
    for node in &loaded.plan.nodes {
        if let ExecutionPlanAction::Execute {
            document: Some(document),
            ..
        } = &node.action
            && child_paths.insert(document.path.as_str())
        {
            evidence_files.push(EvidenceFile {
                display_id: format!("reviewed_document_{:04}", child_paths.len()),
                name: document.path.clone(),
                media_type: "https://www.iana.org/assignments/media-types/application/json"
                    .to_owned(),
                bytes: read_evidence_file(&loaded.directory, &document.path)?,
            });
        }
    }
    let mut profile_paths = BTreeSet::new();
    for requirement in &loaded.plan.requirements {
        if let Some(adapter) = &requirement.adapter
            && profile_paths.insert(adapter.profile_path.as_str())
        {
            evidence_files.push(EvidenceFile {
                display_id: format!("adapter_profile_{:04}", profile_paths.len()),
                name: adapter.profile_path.clone(),
                media_type: "https://www.iana.org/assignments/media-types/application/toml"
                    .to_owned(),
                bytes: read_evidence_file(&loaded.directory, &adapter.profile_path)?,
            });
        }
    }

    let mut triples = loaded
        .inventory
        .document()
        .as_sbol_document()
        .rdf_graph()
        .triples()
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let attachments = evidence_files
        .iter()
        .map(|file| build_attachment(&run_namespace, file))
        .collect::<Result<Vec<_>>>()?;
    let plan_attachment = attachments
        .first()
        .expect("the reviewed plan is always attached")
        .identity
        .clone();
    for attachment in &attachments {
        extend(&mut triples, attachment)?;
    }

    let plan = Plan::builder(run_namespace.as_str(), "reviewed_plan")?
        .name("Reviewed Lab execution plan")
        .description(format!(
            "Exact facility plan with SHA-256 {}",
            loaded.plan_sha256
        ))
        .add_attachment(plan_attachment)
        .build()?;
    let agent = Agent::builder(run_namespace.as_str(), "lab_runtime")?
        .name(format!("Lab runtime {}", env!("CARGO_PKG_VERSION")))
        .build()?;
    extend(&mut triples, &plan)?;
    extend(&mut triples, &agent)?;

    let assets = loaded
        .plan
        .requirements
        .iter()
        .map(|binding| binding.asset.as_str())
        .collect::<BTreeSet<_>>();
    if assets.is_empty() {
        bail!("a completed facility run must use at least one Asset");
    }
    let mut usages = Vec::new();
    for (index, asset) in assets.iter().enumerate() {
        usages.push(
            Usage::builder(&run_resource, format!("asset_{:04}", index + 1))?
                .entity(iri_resource(*asset)?)
                .had_role([Iri::from_static(RUN_ASSET)])
                .build()?,
        );
    }
    for (index, material) in loaded.plan.materials.iter().enumerate() {
        usages.push(
            Usage::builder(&run_resource, format!("input_{:04}", index + 1))?
                .entity(iri_resource(&material.material_lot)?)
                .had_role([Iri::from_static(RUN_INPUT_MATERIAL)])
                .build()?,
        );
    }
    let association = Association::builder(&run_resource, "responsibility")?
        .agent(agent.identity.clone())
        .had_plan(plan.identity.clone())
        .build()?;
    let activity = Activity::builder(run_namespace.as_str(), "run")?
        .name(match mode {
            ExecutionMode::Simulation => "Lab facility simulation",
            ExecutionMode::Live => "Lab facility run",
        })
        .description(format!(
            "{} of reviewed plan {} against facility {}",
            match mode {
                ExecutionMode::Simulation => "Simulation",
                ExecutionMode::Live => "Execution",
            },
            loaded.plan_sha256,
            loaded.plan.inventory.facility
        ))
        .started_at_time(started_at)
        .ended_at_time(ended_at)
        .qualified_usage(usages.iter().map(|usage| usage.identity.clone()))
        .add_qualified_association(association.identity.clone())
        .build()?;
    for usage in &usages {
        extend(&mut triples, usage)?;
    }
    extend(&mut triples, &association)?;
    extend(&mut triples, &activity)?;

    let evidence = ExperimentalData::builder(run_namespace.as_str(), "evidence")?
        .name(match mode {
            ExecutionMode::Simulation => "Lab simulation evidence",
            ExecutionMode::Live => "Lab run evidence",
        })
        .description("Frozen reviewed inputs and the mode-bound durable execution ledger")
        .attachments(
            attachments
                .iter()
                .map(|attachment| attachment.identity.clone()),
        )
        .add_generated_by(activity.identity.clone())
        .build()?;
    extend(&mut triples, &evidence)?;

    let inputs = loaded
        .plan
        .materials
        .iter()
        .map(|material| {
            (
                material.id.as_str(),
                (material.material_lot.as_str(), material.component.as_str()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut output_materials = Vec::new();
    for output in &loaded.plan.outputs {
        let material = build_output_material(
            output,
            &loaded.plan.inventory.facility,
            &activity.identity,
            &inputs,
        )?;
        if loaded
            .inventory
            .document()
            .as_sbol_document()
            .get(&material.identity)
            .is_some()
        {
            bail!(
                "output MaterialLot '{}' already exists in the reviewed inventory",
                output.material_lot
            );
        }
        output_materials.push(output.material_lot.clone());
        extend(&mut triples, &material)?;
    }

    let document = InventoryDocument::from_sbol_document(sbol3::Document::from_rdf_graph(
        RdfGraph::new(triples.into_iter().collect()),
    ));
    document
        .check()
        .map_err(anyhow::Error::new)
        .context("post-run inventory document is not conformant")?;
    let turtle = document.write(RdfFormat::Turtle)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output_path)
        .with_context(|| format!("failed to create {}", output_path.display()))?;
    file.write_all(turtle.as_bytes())
        .with_context(|| format!("failed to write {}", output_path.display()))?;
    file.sync_data()
        .with_context(|| format!("failed to sync {}", output_path.display()))?;

    Ok(InventoryResult {
        path: output_path,
        activity: activity.identity.to_string(),
        evidence: evidence.identity.to_string(),
        output_materials,
    })
}

struct EvidenceFile {
    display_id: String,
    name: String,
    media_type: String,
    bytes: Vec<u8>,
}

fn build_attachment(namespace: &Namespace, file: &EvidenceFile) -> Result<Attachment> {
    let hash = sha256_hex(&file.bytes);
    Ok(
        Attachment::builder(namespace.as_str(), file.display_id.as_str())?
            .name(file.name.clone())
            .source(iri_resource(format!("urn:sha256:{hash}"))?)
            .format(Iri::new(file.media_type.clone())?)
            .size(i64::try_from(file.bytes.len()).context("evidence file is too large")?)
            .hash(hash)
            .hash_algorithm(HashAlgorithm::SHA256)
            .build()?,
    )
}

fn build_output_material(
    output: &ExecutionMaterialOutput,
    facility: &str,
    activity: &Resource,
    inputs: &BTreeMap<&str, (&str, &str)>,
) -> Result<Implementation> {
    let namespace = Namespace::new(output.namespace.clone())?;
    let sources = output
        .derived_from
        .iter()
        .map(|source| {
            inputs
                .get(source.as_str())
                .with_context(|| {
                    format!(
                        "output MaterialLot '{}' derives from unknown input '{}'",
                        output.id, source
                    )
                })
                .and_then(|(material_lot, _)| iri_resource(*material_lot))
        })
        .collect::<Result<Vec<_>>>()?;
    let derived_components = output
        .derived_from
        .iter()
        .map(|source| {
            inputs
                .get(source.as_str())
                .with_context(|| {
                    format!(
                        "output MaterialLot '{}' derives from unknown input '{}'",
                        output.id, source
                    )
                })
                .and_then(|(_, component)| iri_resource(*component))
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let mut builder = Implementation::builder(namespace.as_str(), output.display_id.as_str())?
        .name(output.id.clone())
        .built(iri_resource(&output.component)?)
        .derived_from(derived_components)
        .add_generated_by(activity.clone())
        .extension(
            Iri::from_static(MATERIAL_KIND),
            Term::Resource(iri_resource(&output.material_kind)?),
        )
        .extension(
            Iri::from_static(FACILITY_PROPERTY),
            Term::Resource(iri_resource(facility)?),
        )
        .extension(
            Iri::from_static(IS_ACTIVE),
            Term::Literal(Literal::new("true", Iri::from_static(XSD_BOOLEAN), None)),
        );
    if let Some(location) = &output.located_in {
        builder = builder.extension(
            Iri::from_static(LOCATED_IN),
            Term::Resource(iri_resource(location)?),
        );
    }
    if let Some(position) = &output.position {
        builder = builder.extension(
            Iri::from_static(POSITION),
            Term::Literal(Literal::new(
                position.clone(),
                Iri::from_static(XSD_STRING),
                None,
            )),
        );
    }
    for source in sources {
        builder = builder.extension(
            Iri::from_static(DERIVED_FROM_MATERIAL),
            Term::Resource(source),
        );
    }
    let material = builder.build()?;
    if material.identity.to_string() != output.material_lot {
        bail!(
            "output MaterialLot '{}' does not match its namespace/display_id identity '{}'",
            output.material_lot,
            material.identity
        );
    }
    Ok(material)
}

fn extend(value: &mut BTreeSet<Triple>, object: &impl ToRdf) -> Result<()> {
    value.extend(object.to_rdf_triples()?);
    Ok(())
}

fn read_evidence_file(directory: &Path, relative: &str) -> Result<Vec<u8>> {
    let path = directory.join(relative);
    fs::read(&path).with_context(|| format!("failed to read evidence file {}", path.display()))
}

fn iri_resource(value: impl Into<String>) -> Result<Resource> {
    Ok(Resource::Iri(Iri::new(value.into())?))
}

fn timestamp(value: u64) -> Result<String> {
    let value = i64::try_from(value).context("run timestamp exceeds the supported range")?;
    OffsetDateTime::from_unix_timestamp(value)
        .context("run timestamp is outside the supported date range")?
        .format(&Rfc3339)
        .context("failed to format run timestamp")
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use sbol_inventory::InventoryDocument;
    use sbol_inventory::vocabulary::{
        PROV_ACTIVITY, PROV_WAS_GENERATED_BY, RUN_ASSET, RUN_INPUT_MATERIAL,
    };
    use sbol3::{RdfFormat, Resource};

    use super::*;
    use crate::clock::Clock;
    use crate::execution::{
        load_execution_directory,
        tests::{document_loaders, write_execution_package},
    };
    use crate::ledger::{ExecutionLedger, LedgerEvent};
    use crate::mode::ExecutionMode;

    struct FixedClock;

    impl Clock for FixedClock {
        fn now_unix(&self) -> u64 {
            1_725_000_000
        }
    }

    #[test]
    fn completion_writes_a_new_conformant_inventory_with_run_evidence_and_lineage() {
        let directory = write_execution_package();
        let loaded = load_execution_directory(directory.path(), &document_loaders()).unwrap();
        let source_before = fs::read(loaded.inventory.source_path()).unwrap();
        let nodes = loaded
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<BTreeSet<_>>();
        let mut ledger = ExecutionLedger::create(
            &loaded.directory,
            &loaded.plan_sha256,
            loaded.inventory.source_sha256(),
            nodes,
            ExecutionMode::Live,
            &FixedClock,
        )
        .unwrap();
        for node in &loaded.nodes {
            ledger
                .append(&node.id, LedgerEvent::Started, &FixedClock)
                .unwrap();
            ledger
                .append(&node.id, LedgerEvent::Completed, &FixedClock)
                .unwrap();
        }

        let result =
            write_inventory_result(&loaded, ExecutionMode::Live, 1_725_000_000, 1_725_000_000)
                .unwrap();

        assert_eq!(
            fs::read(loaded.inventory.source_path()).unwrap(),
            source_before
        );
        assert_eq!(result.path, loaded.directory.join(INVENTORY_RESULT_FILE));
        assert_eq!(
            result.output_materials,
            ["https://example.org/results/output_lot"]
        );
        let turtle = fs::read_to_string(&result.path).unwrap();
        let document = InventoryDocument::read(&turtle, RdfFormat::Turtle).unwrap();
        document.check().unwrap();

        let output_identity = iri_resource("https://example.org/results/output_lot").unwrap();
        let output = document.material_lot(&output_identity).unwrap();
        assert_eq!(
            output
                .derived_from_ids()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["https://example.org/facility/input_lot"]
        );
        let implementation = output.as_implementation().unwrap();
        assert!(
            implementation
                .identified
                .generated_by
                .iter()
                .any(|activity| activity.to_string() == result.activity)
        );
        assert!(
            implementation
                .identified
                .derived_from
                .iter()
                .any(|input| input.to_string() == "https://example.org/facility/design")
        );

        let graph = document.as_sbol_document();
        let activity = graph
            .get(&Resource::Iri(Iri::new(result.activity.clone()).unwrap()))
            .unwrap();
        assert!(
            activity
                .rdf_types()
                .iter()
                .any(|kind| kind.as_str() == PROV_ACTIVITY)
        );
        let usage_roles = activity
            .resources("http://www.w3.org/ns/prov#qualifiedUsage")
            .filter_map(|usage| graph.get(usage))
            .flat_map(|usage| usage.iris("http://www.w3.org/ns/prov#hadRole"))
            .map(Iri::as_str)
            .collect::<BTreeSet<_>>();
        assert!(usage_roles.contains(RUN_ASSET));
        assert!(usage_roles.contains(RUN_INPUT_MATERIAL));
        assert!(
            graph
                .objects()
                .values()
                .filter(|object| {
                    object
                        .rdf_types()
                        .iter()
                        .any(|kind| kind.as_str() == "http://sbols.org/v3#Attachment")
                })
                .count()
                >= 4
        );
        let output_object = graph.get(&output_identity).unwrap();
        assert!(
            output_object
                .resources(PROV_WAS_GENERATED_BY)
                .any(|activity| activity.to_string() == result.activity)
        );

        let overwrite =
            write_inventory_result(&loaded, ExecutionMode::Live, 1_725_000_000, 1_725_000_000)
                .unwrap_err()
                .to_string();
        assert!(overwrite.contains("preserving prior run provenance"));
    }
}
