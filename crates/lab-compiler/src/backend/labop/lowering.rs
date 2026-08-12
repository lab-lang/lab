//! Projection from verified Protocol LAIR into LabOP activities.
//!
//! The projection is lossy in one direction only. Protocol LAIR is already
//! verified when it arrives, so nothing here re-checks it; the work is
//! restating each operation in the vocabulary LabOP has, and dropping what
//! LabOP cannot hold. Three losses are structural rather than incidental:
//!
//! - Golden Gate cycling is written out one incubation at a time, because a
//!   LabOP activity has no loop construct.
//! - Replicate counts survive only where a published primitive happens to
//!   declare a `replicates` parameter. Assembly replicates have nowhere to go.
//! - Material identity is not tracked. LabOP object flows carry sample
//!   collections between actions, but nothing marks a collection as consumed,
//!   so the linearity Protocol LAIR guarantees is not restated in the output.
//!
//! Each loss is reported through [`Omission`] rather than passed over, so the
//! emitted bundle can say what the document does not contain.

use pliron::context::Context;

use super::graph::{Document, ProtocolBuilder, Value};
use super::library;
use super::sbol;
use super::vocabulary::Unit;
use crate::backend::trace::{AssemblyTrace, ProtocolTraces, StrainTrace};

/// Something the source protocol states that the emitted document cannot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Omission {
    pub artifact: String,
    pub detail: String,
}

impl Omission {
    fn new(artifact: &str, detail: impl Into<String>) -> Self {
        Self {
            artifact: artifact.to_owned(),
            detail: detail.into(),
        }
    }
}

/// Container requirements the emitted protocols reference, written as the
/// Manchester-syntax class expressions LabOP resolves through a reasoner.
const REACTION_PLATE_QUERY: &str = "cont:ClearPlate and\n cont:SLAS-4-2004 and\n (cont:wellVolume some\n    ((om:hasUnit value om:microlitre) and\n     (om:hasNumericalValue only xsd:decimal[>= \"50\"^^xsd:decimal])))";
const SEAL_QUERY: &str = "cont:SealingFilm";
const AGAR_PLATE_QUERY: &str = "cont:PetriDish";
const CULTURE_PLATE_QUERY: &str = "cont:ClearPlate and\n cont:SLAS-4-2004";

/// What one lowering produced: the protocols now in the document, and what
/// their source stated that they do not.
#[derive(Debug, Default)]
pub(super) struct Lowered {
    pub(super) protocols: Vec<String>,
    pub(super) omissions: Vec<Omission>,
}

impl Lowered {
    fn absorb(&mut self, (protocol, omissions): (String, Vec<Omission>)) {
        self.protocols.push(protocol);
        self.omissions.extend(omissions);
    }
}

pub(super) fn lower(
    document: &mut Document,
    traces: &ProtocolTraces,
    context: &Context,
) -> Lowered {
    let mut lowered = Lowered::default();
    for assembly in &traces.assemblies {
        lowered.absorb(lower_assembly(document, assembly, context));
    }
    for strain in &traces.strains {
        lowered.absorb(lower_strain(document, strain, context));
    }
    lowered
}

fn lower_assembly(
    document: &mut Document,
    trace: &AssemblyTrace,
    context: &Context,
) -> (String, Vec<Omission>) {
    let artifact = trace.artifact(context);
    let mut omissions = Vec::new();
    let mut protocol = document.protocol(
        &format!("{}_assembly", sbol::display_id(&artifact)),
        &format!("{artifact} assembly"),
        &format!(
            "Golden Gate assembly of {artifact} from {} and {} part(s).",
            trace.backbone(context),
            trace.components(context).len()
        ),
    );

    let plate = document.container_spec(
        "reaction_plate",
        "Golden Gate reaction plate",
        REACTION_PLATE_QUERY,
    );
    let reaction = allocate(document, &mut protocol, &plate);

    let reaction_volume = trace.chemistry(context, "reaction_volume_ul");
    let part_volume = trace.chemistry(context, "part_volume_ul");
    let components = trace.components(context);

    let mut reagents = vec![
        (
            "Golden Gate buffer".to_owned(),
            trace.chemistry(context, "buffer_volume_ul"),
        ),
        (
            trace.restriction_enzyme(context),
            trace.chemistry(context, "enzyme_volume_ul"),
        ),
        (
            "T4 DNA ligase".to_owned(),
            trace.chemistry(context, "ligase_volume_ul"),
        ),
        (trace.backbone(context), part_volume),
    ];
    reagents.extend(
        components
            .iter()
            .map(|component| (component.clone(), part_volume)),
    );

    // The reaction balances by construction in verified LAIR, so any remainder
    // is the water the recipe implies rather than a discrepancy to report.
    let consumed: u32 = reagents.iter().map(|(_, volume)| u32::from(*volume)).sum();
    let water = u32::from(reaction_volume).saturating_sub(consumed);
    if water > 0 {
        reagents.push(("nuclease-free water".to_owned(), water as u16));
    }

    for (resource, volume) in reagents {
        provision(document, &mut protocol, &resource, &reaction, volume);
    }

    let mut mix = protocol.action(document, &library::PIPETTE_MIX);
    protocol.input(document, &mut mix, "samples", &reaction);
    protocol.value(
        document,
        &mut mix,
        "amount",
        microlitres(u32::from(reaction_volume) / 2),
    );
    protocol.commit(document, mix);

    let seal = document.container_spec("adhesive_seal", "Adhesive plate seal", SEAL_QUERY);
    let mut seal_action = protocol.action(document, &library::SEAL);
    protocol.input(document, &mut seal_action, "location", &reaction);
    protocol.value(
        document,
        &mut seal_action,
        "specification",
        Value::Reference(seal),
    );
    protocol.commit(document, seal_action);

    let cycles = trace.chemistry(context, "cycles");
    for _ in 0..cycles {
        incubate(
            document,
            &mut protocol,
            &reaction,
            trace.chemistry(context, "digest_minutes"),
            trace.chemistry(context, "digest_temperature_c"),
        );
        incubate(
            document,
            &mut protocol,
            &reaction,
            trace.chemistry(context, "ligate_minutes"),
            trace.chemistry(context, "ligate_temperature_c"),
        );
    }
    if cycles > 1 {
        omissions.push(Omission::new(
            &artifact,
            format!(
                "{cycles} thermal cycles are written as {} separate incubations because a LabOP activity has no loop construct",
                cycles * 2
            ),
        ));
    }

    let mut assemble = protocol.action(document, &library::ASSEMBLE);
    protocol.input(document, &mut assemble, "reaction", &reaction);
    let backbone = document.component(&trace.backbone(context));
    protocol.value(
        document,
        &mut assemble,
        "backbone",
        Value::Reference(backbone),
    );
    for component in &components {
        let part = document.component(component);
        protocol.value(document, &mut assemble, "parts", Value::Reference(part));
    }
    let enzyme = document.component(&trace.restriction_enzyme(context));
    protocol.value(
        document,
        &mut assemble,
        "restriction_enzyme",
        Value::Reference(enzyme),
    );
    let product = protocol.output(document, &mut assemble, "product");
    protocol.commit(document, assemble);

    let mut accept = protocol.action(document, &library::ACCEPT);
    protocol.input(document, &mut accept, "samples", &product);
    protocol.value(
        document,
        &mut accept,
        "artifact",
        Value::Text(artifact.clone()),
    );
    if let Some(minimum) = trace.minimum_concentration_ng_per_ul(context) {
        // OM-2 has no nanogram-per-microlitre resource, so the threshold is
        // carried as a plain number and its unit stated in the description.
        protocol.value(
            document,
            &mut accept,
            "minimum_concentration",
            Value::Integer(i64::from(minimum)),
        );
    }
    if let Some(minimum) = trace.minimum_volume_ul(context) {
        protocol.value(
            document,
            &mut accept,
            "minimum_volume",
            microlitres(minimum),
        );
    }
    protocol.commit(document, accept);
    let display_id = protocol.finish(document);

    let replicates = trace.assembly_replicates(context);
    if replicates > 1 {
        omissions.push(Omission::new(
            &artifact,
            format!(
                "{replicates} assembly replicates are not represented; no LabOP assembly primitive declares a replicate count"
            ),
        ));
    }
    let dependencies = trace.dependencies(context);
    if !dependencies.is_empty() {
        omissions.push(Omission::new(
            &artifact,
            format!(
                "dependencies on {} are not represented; LabOP has no cross-protocol material reference",
                dependencies.join(", ")
            ),
        ));
    }
    omissions.push(Omission::new(
        &artifact,
        "material linearity is not represented; LabOP object flows do not mark a sample as consumed",
    ));
    (display_id, omissions)
}

fn lower_strain(
    document: &mut Document,
    trace: &StrainTrace,
    context: &Context,
) -> (String, Vec<Omission>) {
    let artifact = trace.artifact(context);
    let mut omissions = Vec::new();
    let host = trace.host(context);
    let plasmids = trace.plasmids(context);
    let selection = trace.selection(context);

    let mut protocol = document.protocol(
        &format!("{}_strain", sbol::display_id(&artifact)),
        &format!("{artifact} construction"),
        &format!(
            "Heat-shock transformation of {host} with {}, recovery, dilution, and selection on {selection}.",
            plasmids.join(", ")
        ),
    );

    let tube = document.container_spec(
        "transformation_plate",
        "Transformation plate",
        CULTURE_PLATE_QUERY,
    );
    let reaction = allocate(document, &mut protocol, &tube);

    let host_component = document.component(&host);
    let medium = document.component("SOC recovery medium");
    let selection_component = document.component(&selection);

    let mut transform = protocol.action(document, &library::TRANSFORM);
    protocol.value(
        document,
        &mut transform,
        "host",
        Value::Reference(host_component),
    );
    for plasmid in &plasmids {
        let dna = document.component(plasmid);
        protocol.value(document, &mut transform, "dna", Value::Reference(dna));
    }
    protocol.value(
        document,
        &mut transform,
        "amount",
        microlitres(u32::from(trace.chemistry(context, "dna_volume_ul"))),
    );
    protocol.value(
        document,
        &mut transform,
        "selection_medium",
        Value::Reference(selection_component.clone()),
    );
    protocol.input(document, &mut transform, "destination", &reaction);
    let transformants = protocol.output(document, &mut transform, "transformants");
    protocol.commit(document, transform);

    incubate(
        document,
        &mut protocol,
        &transformants,
        trace.chemistry(context, "cold_minutes"),
        4,
    );
    incubate(
        document,
        &mut protocol,
        &transformants,
        trace.chemistry(context, "heat_shock_minutes"),
        trace.chemistry(context, "heat_shock_temperature_c"),
    );
    provision(
        document,
        &mut protocol,
        "SOC recovery medium",
        &transformants,
        trace.chemistry(context, "recovery_volume_ul"),
    );
    incubate(
        document,
        &mut protocol,
        &transformants,
        trace.chemistry(context, "recovery_minutes"),
        trace.chemistry(context, "recovery_temperature_c"),
    );

    let dilutions = trace.serial_dilutions(context);
    if dilutions > 0 {
        let mut dilute = protocol.action(document, &library::SERIAL_DILUTION);
        protocol.input(document, &mut dilute, "samples", &transformants);
        protocol.value(
            document,
            &mut dilute,
            "direction",
            Value::Text("row".into()),
        );
        protocol.value(document, &mut dilute, "diluent", Value::Reference(medium));
        protocol.value(
            document,
            &mut dilute,
            "amount",
            microlitres(u32::from(trace.chemistry(context, "medium_volume_ul"))),
        );
        protocol.value(document, &mut dilute, "dilution_factor", Value::Integer(10));
        protocol.commit(document, dilute);
        omissions.push(Omission::new(
            &artifact,
            format!(
                "the {dilutions} serial dilution steps collapse into one SerialDilution action; LabOP does not record the resulting series as distinct materials"
            ),
        ));
    }

    let agar = document.container_spec("agar_plate", "Selective agar plate", AGAR_PLATE_QUERY);
    let plating_replicates = trace.plating_replicates(context);
    let mut plates = protocol.action(document, &library::CULTURE_PLATES);
    protocol.value(
        document,
        &mut plates,
        "quantity",
        Value::Integer(i64::from(plating_replicates)),
    );
    protocol.value(
        document,
        &mut plates,
        "specification",
        Value::Reference(agar),
    );
    protocol.value(
        document,
        &mut plates,
        "replicates",
        Value::Integer(i64::from(plating_replicates)),
    );
    protocol.value(
        document,
        &mut plates,
        "growth_medium",
        Value::Reference(selection_component),
    );
    let plate_samples = protocol.output(document, &mut plates, "samples");
    protocol.commit(document, plates);

    let mut spread = protocol.action(document, &library::TRANSFER);
    protocol.input(document, &mut spread, "source", &transformants);
    protocol.input(document, &mut spread, "destination", &plate_samples);
    protocol.value(
        document,
        &mut spread,
        "amount",
        microlitres(u32::from(trace.chemistry(context, "culture_volume_ul"))),
    );
    protocol.value(
        document,
        &mut spread,
        "replicates",
        Value::Integer(i64::from(plating_replicates)),
    );
    protocol.commit(document, spread);

    incubate(document, &mut protocol, &plate_samples, 960, 37);
    let display_id = protocol.finish(document);

    let transformation_replicates = trace.transformation_replicates(context);
    if transformation_replicates > 1 {
        omissions.push(Omission::new(
            &artifact,
            format!(
                "{transformation_replicates} transformation replicates are not represented; the LabOP Transform primitive declares no replicate count"
            ),
        ));
    }
    omissions.push(Omission::new(
        &artifact,
        "whether the plated colonies are biological or technical replicates is not represented; LabOP records no lineage",
    ));
    (display_id, omissions)
}

/// A fresh container and the sample collection it holds.
fn allocate(
    document: &mut Document,
    protocol: &mut ProtocolBuilder,
    specification: &str,
) -> String {
    let mut action = protocol.action(document, &library::EMPTY_CONTAINER);
    protocol.value(
        document,
        &mut action,
        "specification",
        Value::Reference(specification.to_owned()),
    );
    let samples = protocol.output(document, &mut action, "samples");
    protocol.commit(document, action);
    samples
}

fn provision(
    document: &mut Document,
    protocol: &mut ProtocolBuilder,
    resource: &str,
    destination: &str,
    volume_ul: u16,
) {
    let component = document.component(resource);
    let mut action = protocol.action(document, &library::PROVISION);
    protocol.value(
        document,
        &mut action,
        "resource",
        Value::Reference(component),
    );
    protocol.input(document, &mut action, "destination", destination);
    protocol.value(
        document,
        &mut action,
        "amount",
        microlitres(u32::from(volume_ul)),
    );
    protocol.commit(document, action);
}

fn incubate(
    document: &mut Document,
    protocol: &mut ProtocolBuilder,
    location: &str,
    minutes: u16,
    celsius: u16,
) {
    let mut action = protocol.action(document, &library::INCUBATE);
    protocol.input(document, &mut action, "location", location);
    protocol.value(
        document,
        &mut action,
        "duration",
        Value::Measure {
            amount: f64::from(minutes),
            unit: Unit::Minute,
        },
    );
    protocol.value(
        document,
        &mut action,
        "temperature",
        Value::Measure {
            amount: f64::from(celsius),
            unit: Unit::Celsius,
        },
    );
    protocol.commit(document, action);
}

fn microlitres(amount: u32) -> Value {
    Value::Measure {
        amount: f64::from(amount),
        unit: Unit::Microlitre,
    }
}
