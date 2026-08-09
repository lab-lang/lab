//! The operator document: deck loading, source fills, and the interleaved
//! sequence of robot runs and manual steps.

use std::fmt::Write;

use crate::backend::hamilton::star::catalog;
use crate::backend::hamilton::star::plan::StarExecutionPlan;

pub(in crate::backend::hamilton::star) fn render_manual_protocol(
    plan: &StarExecutionPlan,
) -> String {
    let mut output = String::new();
    let profile = &plan.deck;
    writeln!(output, "# Lab Hamilton STAR run — human instructions\n").unwrap();
    writeln!(output, "> Generated concept protocol. Review and qualify every run for the actual laboratory before execution. Planning success is not physical-build or acceptance evidence.\n").unwrap();
    writeln!(
        output,
        "Compiled for bench `{}` ({} deck, {} channels). Robot steps live in the `*.star.json` run documents beside this file; `lab run` replays them frame for frame.\n",
        profile.target.name,
        profile.machine.variant.name(),
        profile.machine.channels,
    )
    .unwrap();

    writeln!(output, "## Deck loading\n").unwrap();
    writeln!(output, "| Carrier | Catalog model | Rail |").unwrap();
    writeln!(output, "| --- | --- | ---: |").unwrap();
    for (name, placement) in &profile.deck.carriers {
        let model = catalog::carrier(&placement.catalog)
            .map(|carrier| carrier.hamilton_model)
            .unwrap_or("unknown");
        writeln!(output, "| {name} | {model} | {} |", placement.rail).unwrap();
    }
    writeln!(output).unwrap();

    writeln!(output, "| Resource | Site | Labware |").unwrap();
    writeln!(output, "| --- | --- | --- |").unwrap();
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
    for (resource, site, labware) in labware_rows {
        writeln!(output, "| {resource} | {site} | {labware} |").unwrap();
    }
    writeln!(output).unwrap();

    writeln!(output, "## Source loading\n").unwrap();
    writeln!(output, "Load each position with at least the stated volume (consumption plus the vessel's dead volume). The source rack is reloaded between the assembly and transformation stages.\n").unwrap();
    writeln!(output, "| Stage | Contents | Position | Load (µL) |").unwrap();
    writeln!(output, "| --- | --- | --- | ---: |").unwrap();
    for fill in &plan.source_fills {
        let stage = match fill.location.resource.as_str() {
            "assembly_sources" => "assembly",
            "transformation_sources" => "transformation",
            "media_rack" => "plating",
            _ => "transformation",
        };
        writeln!(
            output,
            "| {stage} | {} | {} {} | {:.0} |",
            fill.key,
            fill.location.resource,
            fill.location.well,
            fill.load_ul.ceil(),
        )
        .unwrap();
    }
    writeln!(output).unwrap();

    if !plan.tip_usage.is_empty() {
        writeln!(output, "## Tip consumption\n").unwrap();
        writeln!(output, "| Rack | Tips used |").unwrap();
        writeln!(output, "| --- | ---: |").unwrap();
        for (rack, used) in &plan.tip_usage {
            writeln!(output, "| {rack} | {used} |").unwrap();
        }
        writeln!(output).unwrap();
    }

    writeln!(output, "## Run sequence\n").unwrap();
    writeln!(
        output,
        "Execute the runs in order; complete every manual step before starting the next run.\n"
    )
    .unwrap();
    for (index, run) in plan.runs.iter().enumerate() {
        writeln!(
            output,
            "### {}. {} (`{}.star.json`)\n",
            index + 1,
            run.title,
            run.id
        )
        .unwrap();
        writeln!(
            output,
            "{} machine operations. Start the run with `lab run`; the machine confirms each frame before the next.\n",
            run.operations.len()
        )
        .unwrap();
        for step in &run.manual_after {
            writeln!(
                output,
                "**Then, by hand — {}:** {}\n",
                step.title, step.instructions
            )
            .unwrap();
        }
    }

    output
}

fn display_labware(id: &str) -> String {
    catalog::labware(id)
        .map(|labware| labware.display.to_string())
        .unwrap_or_else(|| id.to_string())
}
