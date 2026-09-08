//! The standard library's portable Method catalog.
//!
//! Built-in Methods use the same checked-in `lab.method-catalog.v2` document and parser as a
//! package dependency. Rust supplies only the default application composition; scientific Method
//! definitions do not receive a privileged constructor API here.

use std::sync::OnceLock;

use crate::method::{MethodCatalogDocument, MethodDefinition, MethodRegistry};

const STANDARD_METHOD_CATALOG: &str = include_str!("../../catalogs/standard-methods.json");

/// Return the validated standard Method catalog bundled with this compiler build.
pub fn standard_method_catalog() -> &'static MethodCatalogDocument {
    static CATALOG: OnceLock<MethodCatalogDocument> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let catalog = serde_json::from_str::<MethodCatalogDocument>(STANDARD_METHOD_CATALOG)
            .expect("the checked-in standard Method catalog parses");
        catalog
            .validate()
            .expect("the checked-in standard Method catalog is valid");
        catalog
    })
}

/// Return the validated standard Method registry bundled with this compiler build.
pub fn standard_method_registry() -> &'static MethodRegistry {
    static REGISTRY: OnceLock<MethodRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        MethodRegistry::new(standard_method_catalog().methods.clone())
            .expect("the checked-in standard Method catalog is valid")
    })
}

/// Return owned definitions for application compositions that add package Methods.
pub fn standard_method_definitions() -> Vec<MethodDefinition> {
    standard_method_catalog().methods.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_methods_are_an_ordinary_round_tripping_catalog() {
        let catalog = standard_method_catalog();
        assert_eq!(catalog.methods.len(), 18);
        assert!(
            catalog
                .methods
                .iter()
                .any(|method| method.refines.as_str() == "std.lab.plasmid.split")
        );
        assert!(
            catalog
                .methods
                .iter()
                .any(|method| method.refines.as_str() == "std.lab.plasmid.dispose")
        );
        let serialized = serde_json::to_string_pretty(catalog).unwrap();
        let reparsed = serde_json::from_str::<MethodCatalogDocument>(&serialized).unwrap();
        reparsed.validate().unwrap();
        assert_eq!(reparsed, *catalog);
        assert_eq!(
            standard_method_registry().definitions().count(),
            catalog.methods.len()
        );
    }
}
