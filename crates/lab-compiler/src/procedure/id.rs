//! Stable identities shared by enclosing Method graphs and task-interior Procedure programs.

pub use crate::method::{LocalId as ProcedureLocalId, LocalIdError as ProcedureLocalIdError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedure_and_method_ids_are_one_type() {
        let procedure = ProcedureLocalId::new("reaction/1").unwrap();
        let method: crate::method::LocalId = procedure;
        assert_eq!(method.as_str(), "reaction/1");
    }
}
