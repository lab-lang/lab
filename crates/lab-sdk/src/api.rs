use lab_compiler::CheckedModule;

use crate::Error;

/// Parse, resolve, and type-check a Lab source module.
pub fn compile_lab_module(source: &str) -> Result<CheckedModule, Error> {
    Ok(lab_compiler::compile_module(source)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_to_checked_module_sdk_path_is_coherent() {
        let source = r#"
plasmid p_sdk:
  sequence: dna("ACGT")
  accept sequence == design.sequence
        "#;
        let module = compile_lab_module(source).unwrap();
        assert_eq!(module.declarations.len(), 1);
    }
}
