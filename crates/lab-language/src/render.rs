use crate::{CheckedDeclaration, CheckedModule};

/// Render the verified portable module boundary without implying physical
/// execution or target selection.
pub fn render_checked_module(module: &CheckedModule) -> String {
    let mut output = String::from("Lab module compiled\n\n");
    if !module.imports.is_empty() {
        output.push_str("Resolved imports\n");
        for import in &module.imports {
            output.push_str(&format!("  - {} ({})\n", import.module, import.provider));
        }
        output.push('\n');
    }
    output.push_str("Verified declarations\n");
    for declaration in &module.declarations {
        match declaration {
            CheckedDeclaration::Circuit {
                name, output: ty, ..
            } => {
                output.push_str(&format!("  - circuit {name} -> {ty}\n"));
            }
            CheckedDeclaration::Artifact {
                artifact,
                name,
                requirements,
                acceptance,
                ..
            } => output.push_str(&format!(
                "  - {} {name} ({} requirements, {} acceptance claims)\n",
                artifact.keyword(),
                requirements.len(),
                acceptance.len()
            )),
            CheckedDeclaration::Data { category, name, .. } => {
                output.push_str(&format!("  - {category} {name}\n"))
            }
            CheckedDeclaration::Workflow { name, outputs, .. } => {
                let rendered = if let [result] = outputs.as_slice()
                    && result.name == "outcome"
                {
                    result.r#type.to_string()
                } else {
                    format!(
                        "({})",
                        outputs
                            .iter()
                            .map(|field| format!("{}: {}", field.name, field.r#type))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                output.push_str(&format!("  - workflow {name} -> {rendered}\n"));
            }
            CheckedDeclaration::Binding(binding) => {
                output.push_str(&format!(
                    "  - binding {}\n",
                    binding
                        .targets
                        .iter()
                        .map(|target| target.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }
    output.push_str(
        "\nThis is verified portable module IR; no laboratory target was selected or executed.\n",
    );
    output
}
