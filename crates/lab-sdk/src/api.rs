use lab_compiler::LabProfile;
use lab_compiler::{Compilation, Compiler};

use crate::Error;

/// Parse Lab Lang and compile it for a laboratory target.
pub fn compile_lab_lang(source: &str, lab: &LabProfile) -> Result<Compilation, Error> {
    let specification = lab_compiler::parse(source)?;
    Ok(Compiler.compile(&specification, lab)?)
}

#[cfg(test)]
mod tests {
    use lab_compiler::LabProfile;

    use super::*;

    #[test]
    fn source_to_plan_sdk_path_is_coherent() {
        let source = r#"
plasmid p_sdk:
  sequence = dna("ACGT")
  accept sequence == design.sequence
        "#;
        let compilation = compile_lab_lang(source, &LabProfile::reference()).unwrap();
        assert_eq!(compilation.plan().artifact, "p_sdk");
        assert!(compilation.ir().contains("protocol.accept"));
    }
}
