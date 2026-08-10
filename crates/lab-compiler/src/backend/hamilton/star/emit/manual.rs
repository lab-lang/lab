//! The operator document, in two fragments. Bench sections describe the
//! machine, how to drive it, and the deck it was compiled against; run
//! sections carry one execution plan's source fills, tip budget, and run
//! sequence. A standalone manual composes both, and the stitched full-build
//! document renders the bench sections once.

use crate::backend::document::{Block, Column, Doc, DocMeta, bold, code, text};
use crate::backend::hamilton::star::catalog;
use crate::backend::hamilton::star::plan::StarExecutionPlan;
use crate::backend::hamilton::star::profile::StarTargetProfile;

fn fragment() -> Doc {
    Doc::new(DocMeta::new("", "", "", ""))
}

/// The machine, the runner, and the deck: everything that holds for any run
/// compiled against this bench profile.
pub(in crate::backend) fn bench_blocks(profile: &StarTargetProfile) -> Vec<Block> {
    let mut doc = fragment();
    doc.para([
        text(format!(
            "Compiled for bench {} ({} deck, {} channels). Robot steps live in the ",
            profile.target.name,
            profile.machine.variant.name(),
            profile.machine.channels,
        )),
        code("*.star.json"),
        text(" run documents beside this file; "),
        code("lab run"),
        text(" replays them frame for frame."),
    ]);

    doc.heading(1, [text("How Lab and the machine divide the work")]);
    doc.para([
        text("This manual, the run documents, and the automation manifest were generated together by "),
        code("lab build"),
        text(" as projections of one execution plan: the deck positions, source volumes, and tip counts below are exactly what the run documents execute. Regenerate the package from the "),
        code(".lab"),
        text(" sources rather than editing any generated file."),
    ]);
    doc.para_text(
        "The machine performs the liquid handling: reagent distribution, reaction setup, transformation mixes, dilution, and plating transfers. Temperature work and everything between runs stays with the operator. The run sequence below interleaves each machine run with the manual steps that follow it, in the exact order to perform them.",
    );

    doc.heading(1, [text("Running this package")]);
    doc.heading(2, [text("Connecting to the machine")]);
    doc.para_text(
        "Power up the STAR and let its firmware finish initializing before connecting; the machine reports ready once its lamps settle. Connect the instrument to the workstation over USB, and make sure no other software holds the connection, since the runner claims the device exclusively while a run is live.",
    );
    doc.heading(2, [text("Starting a run")]);
    doc.para([
        text("Start each run from this directory with "),
        code("lab run <this directory>"),
        text(". The runner validates the package, prints the full step table, and asks for confirmation before touching hardware. During a run the machine confirms every frame before the next is sent, and manual steps prompt in the terminal for the operator to acknowledge. Nothing moves until the confirmation is given, so it is safe to read the table first and start when the deck is ready."),
    ]);
    doc.para([
        text("To rehearse without hardware, use "),
        code("lab run --dry-run"),
        text(", which validates the package and prints the same step table while leaving the machine untouched."),
    ]);

    doc.heading(1, [text("Deck loading")]);
    doc.para_text(
        "Load the carriers on the rails listed below, then the labware into their sites. The deck stays in this configuration for every run in the session; only the source rack is reloaded between stages.",
    );
    doc.table(
        [
            Column::left("Carrier"),
            Column::left("Catalog model"),
            Column::right("Rail"),
        ],
        profile.deck.carriers.iter().map(|(name, placement)| {
            let model = catalog::carrier(&placement.catalog)
                .map(|carrier| carrier.hamilton_model)
                .unwrap_or("unknown");
            vec![
                vec![text(name.as_str())],
                vec![text(model)],
                vec![text(placement.rail.to_string())],
            ]
        }),
    );

    let mut labware_rows: Vec<(String, String, String)> = vec![
        (
            "source rack (reloaded per stage)".into(),
            profile.deck.source_rack.site.clone(),
            display_labware(&profile.deck.source_rack.labware),
        ),
        (
            "reaction plate".into(),
            profile.deck.reaction_plate.site.clone(),
            display_labware(&profile.deck.reaction_plate.labware),
        ),
        (
            "media".into(),
            profile.stages.plating.media_rack.slot.clone(),
            display_labware(&profile.stages.plating.media_rack.labware),
        ),
    ];
    for (label, plates) in [
        ("DNA plate", &profile.stages.transformation.dna_plate),
        ("dilution plate", &profile.stages.plating.dilution_plate),
        ("agar plate", &profile.stages.plating.agar_plate),
    ] {
        for (index, slot) in plates.slots.iter().enumerate() {
            labware_rows.push((
                format!("{label} {}", index + 1),
                slot.clone(),
                display_labware(&plates.labware),
            ));
        }
    }
    for (label, racks) in [
        ("assembly small tips", &profile.stages.assembly.small_tips),
        (
            "transformation small tips",
            &profile.stages.transformation.small_tips,
        ),
        (
            "transformation large tips",
            &profile.stages.transformation.large_tips,
        ),
        ("plating small tips", &profile.stages.plating.small_tips),
        ("plating large tips", &profile.stages.plating.large_tips),
    ] {
        for (index, slot) in racks.slots.iter().enumerate() {
            labware_rows.push((
                format!("{label} {}", index + 1),
                slot.clone(),
                display_labware(&racks.labware),
            ));
        }
    }
    doc.table(
        [
            Column::left("Resource"),
            Column::left("Site"),
            Column::left("Labware"),
        ],
        labware_rows.into_iter().map(|(resource, site, labware)| {
            vec![vec![text(resource)], vec![text(site)], vec![text(labware)]]
        }),
    );

    doc.blocks
}

/// One execution plan's own content: what to load, how many tips it burns,
/// and the run sequence to work through.
pub(in crate::backend) fn run_blocks(plan: &StarExecutionPlan) -> Vec<Block> {
    let mut doc = fragment();

    doc.heading(1, [text("Source loading")]);
    doc.para_text(
        "Load each position with at least the stated volume (consumption plus the vessel's dead volume). The source rack is reloaded between the assembly and transformation stages.",
    );
    doc.table(
        [
            Column::left("Stage"),
            Column::left("Contents"),
            Column::left("Position"),
            Column::right("Load (µL)"),
        ],
        plan.source_fills.iter().map(|fill| {
            let stage = match fill.location.resource.as_str() {
                "assembly_sources" => "assembly",
                "transformation_sources" => "transformation",
                "media_rack" => "plating",
                _ => "transformation",
            };
            vec![
                vec![text(stage)],
                vec![text(fill.key.as_str())],
                vec![text(format!(
                    "{} {}",
                    fill.location.resource, fill.location.well
                ))],
                vec![text(format!("{:.0}", fill.load_ul.ceil()))],
            ]
        }),
    );

    if !plan.tip_usage.is_empty() {
        doc.heading(1, [text("Tip consumption")]);
        doc.table(
            [Column::left("Rack"), Column::right("Tips used")],
            plan.tip_usage
                .iter()
                .map(|(rack, used)| vec![vec![text(rack.as_str())], vec![text(used.to_string())]]),
        );
    }

    doc.heading(1, [text("Run sequence")]);
    doc.para_text(
        "Execute the runs in order; complete every manual step before starting the next run.",
    );
    for (index, run) in plan.runs.iter().enumerate() {
        doc.labeled_heading(
            2,
            format!("Run {}", index + 1),
            [
                text(format!("{} (", run.title)),
                code(format!("{}.star.json", run.id)),
                text(")"),
            ],
        );
        doc.para([
            text(format!(
                "{} machine operations. Start the run with ",
                run.operations.len()
            )),
            code("lab run"),
            text("; the machine confirms each frame before the next."),
        ]);
        for step in &run.manual_after {
            doc.para([
                bold(format!("Then, by hand ({}):", step.title)),
                text(format!(" {}", step.instructions)),
            ]);
        }
    }

    doc.blocks
}

pub(in crate::backend) fn render_manual_protocol(plan: &StarExecutionPlan) -> Doc {
    let profile = &plan.deck;
    let mut doc = Doc::new(DocMeta::new(
        "Hamilton STAR run",
        "Operator instructions for one machine session",
        &profile.target.name,
        "Hamilton STAR",
    ));
    doc.notice([text(
        "Generated concept protocol. Review and qualify every run for the actual laboratory before execution. Planning success is not physical-build or acceptance evidence.",
    )]);
    doc.blocks.extend(bench_blocks(profile));
    doc.blocks.extend(run_blocks(plan));
    doc
}

fn display_labware(id: &str) -> String {
    catalog::labware(id)
        .map(|labware| labware.display.to_string())
        .unwrap_or_else(|| id.to_string())
}
