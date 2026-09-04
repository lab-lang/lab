//! The operator run sheet for a reviewed plan's manual steps.
//!
//! A step an instrument runs arrives with its own operator document, rendered
//! by the adapter that lowered it. A manual-control step has no adapter, so
//! this is where its instructions land: one document for the whole plan, each
//! step stating the operation it performs, the asset it uses, and the
//! parameters it runs at.

use crate::backend::document::{Block, Column, Doc, DocMeta, code, text};
use crate::backend::typst;

/// Everything a manual run sheet says.
pub struct RunSheet {
    /// The package whose entry workflow the plan runs.
    pub package: String,
    pub version: String,
    /// The facility the plan allocated against, as display text.
    pub facility: String,
    /// The manual steps, in the order the plan performs them.
    pub steps: Vec<RunStep>,
}

/// One manual step of the plan.
pub struct RunStep {
    /// What the operator does, e.g. "Centrifuge".
    pub title: String,
    /// The exact Procedure operation the step performs.
    pub operation: String,
    /// The asset the step uses, as display text.
    pub asset: String,
    /// Display-ready parameter names and values.
    pub parameters: Vec<(String, String)>,
}

/// The style sheet a rendered run sheet imports, written beside the source as
/// [`RUN_SHEET_STYLE_PATH`] so the directory typesets standalone.
pub const RUN_SHEET_STYLE: &str = typst::STYLE;

/// The file name the style sheet is written under.
pub const RUN_SHEET_STYLE_PATH: &str = typst::STYLE_PATH;

/// Render the run sheet as Typst source.
pub fn render_run_sheet(sheet: &RunSheet) -> String {
    let mut doc = Doc::new(DocMeta::new(
        "Manual protocol",
        format!("Operator run sheet for {} {}", sheet.package, sheet.version),
        "",
        sheet.facility.clone(),
    ));
    doc.notice([text(
        "Generated from the reviewed facility plan. Perform each step in order and \
         confirm it in the run ledger.",
    )]);
    for (index, step) in sheet.steps.iter().enumerate() {
        doc.blocks.push(Block::Heading {
            level: 1,
            label: Some(format!("Step {}", index + 1)),
            text: vec![text(step.title.clone())],
        });
        doc.para([
            text("Perform "),
            code(step.operation.clone()),
            text(" using "),
            code(step.asset.clone()),
            text("."),
        ]);
        doc.table(
            [Column::left("Parameter"), Column::left("Value")],
            step.parameters
                .iter()
                .map(|(name, value)| vec![vec![code(name.clone())], vec![text(value.clone())]]),
        );
    }
    typst::render(&doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_each_step_with_its_parameters() {
        let source = render_run_sheet(&RunSheet {
            package: "gll".to_owned(),
            version: "0.1.0".to_owned(),
            facility: "Genetic Logic Lab".to_owned(),
            steps: vec![RunStep {
                title: "Centrifuge".to_owned(),
                operation: "https://www.lab-compiler.org/ns/procedure#Centrifuge".to_owned(),
                asset: "bench_workstation".to_owned(),
                parameters: vec![("force".to_owned(), "4000 rcf".to_owned())],
            }],
        });
        assert!(
            source.contains("Operator run sheet for gll 0.1.0"),
            "{source}"
        );
        assert!(source.contains("Step 1"), "{source}");
        assert!(source.contains("4000 rcf"), "{source}");
        assert!(
            source.contains("#import \"lab-style.typ\""),
            "the sheet typesets against the bundled style: {source}"
        );
    }
}
