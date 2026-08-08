//! Identity conversions: LSP URIs, filesystem paths, workspace source ids,
//! and manifest-derived module names.

use std::path::{Path, PathBuf};

use lab_language::{ModuleId, SourceId};
use lab_package::LabPackage;
use lsp_types as lsp;

pub(crate) fn source_id(uri: &lsp::Uri) -> SourceId {
    SourceId::new(uri.as_str())
}

/// A `file:` URI as a filesystem path, with percent escapes decoded. Any other
/// scheme names something that is not on disk and has no package.
pub(crate) fn uri_to_path(uri: &lsp::Uri) -> Option<PathBuf> {
    file_uri_to_path(uri.as_str())
}

/// The path behind a workspace source identity, which is always formed as a
/// `file:` URI (see `path_to_source_id`).
pub(crate) fn source_id_to_path(source: &SourceId) -> Option<PathBuf> {
    file_uri_to_path(&source.0)
}

fn file_uri_to_path(text: &str) -> Option<PathBuf> {
    let encoded = text.strip_prefix("file://")?;
    let encoded = encoded.strip_prefix("localhost").unwrap_or(encoded);
    let mut decoded = String::with_capacity(encoded.len());
    let mut bytes = encoded.bytes().enumerate();
    while let Some((index, byte)) = bytes.next() {
        if byte != b'%' {
            decoded.push(char::from(byte));
            continue;
        }
        let hex = encoded.get(index + 1..index + 3)?;
        let value = u8::from_str_radix(hex, 16).ok()?;
        decoded.push(char::from(value));
        bytes.next();
        bytes.next();
    }
    Some(PathBuf::from(decoded))
}

/// The same `file:` URI a client would send for this path, so a document loaded
/// from disk and the same document opened in the editor share one identity.
pub(crate) fn path_to_source_id(path: &Path) -> SourceId {
    SourceId::new(format!("file://{}", path.display()))
}

/// The manifest-derived module name for one file of a package.
pub(crate) fn package_module_id(package: &LabPackage, path: &Path) -> Option<ModuleId> {
    let canonical = path.canonicalize().ok();
    package
        .sources
        .iter()
        .find(|source| {
            source.path == path
                || canonical
                    .as_ref()
                    .is_some_and(|canonical| &source.path == canonical)
        })
        .map(|source| ModuleId::new(source.module.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(path: &str) -> lsp::Uri {
        format!("file://{path}").parse().unwrap()
    }

    #[test]
    fn a_file_in_a_package_takes_its_module_name_from_the_manifest() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/golden-gate/src/designs/inventory.lab")
            .canonicalize()
            .unwrap();
        let package = LabPackage::discover(&path).unwrap();

        assert_eq!(
            package_module_id(&package, &path).unwrap().as_str(),
            "golden_gate.designs.inventory",
            "the manifest's package name namespaces the module, not the path on disk"
        );
    }

    #[test]
    fn decodes_percent_escapes_in_file_uris() {
        assert_eq!(
            uri_to_path(&uri("/tmp/a%20b/c.lab")).unwrap(),
            PathBuf::from("/tmp/a b/c.lab")
        );
        assert_eq!(uri_to_path(&"memory:test.lab".parse().unwrap()), None);
    }
}
