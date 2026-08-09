//! Keeping the workspace in step with the world: open-buffer lifecycle,
//! package seeding, and file events from outside the editor.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::Path;

use lab_language::ModuleId;
use lab_package::LabPackage;
use lsp_server::Notification;
use lsp_types as lsp;

use crate::paths::{
    package_module_id, path_to_source_id, source_id, source_id_to_path, uri_to_path,
};
use crate::server::{Server, params};

impl Server {
    pub(crate) fn handle_notification(
        &mut self,
        notification: Notification,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                let params: lsp::DidOpenTextDocumentParams = params(notification.params)?;
                let source = source_id(&params.text_document.uri);
                self.load_package_for(&params.text_document.uri);
                let version = i64::from(params.text_document.version);
                match self.module_id(&params.text_document.uri) {
                    Some(module) => self.workspace.set_module_document(
                        source.clone(),
                        version,
                        params.text_document.text,
                        module,
                    ),
                    None => self.workspace.set_document(
                        source.clone(),
                        version,
                        params.text_document.text,
                    ),
                }
                self.open_documents.insert(source, params.text_document.uri);
                self.publish_open_diagnostics()?;
            }
            "textDocument/didChange" => {
                let params: lsp::DidChangeTextDocumentParams = params(notification.params)?;
                if let Some(change) = params.content_changes.into_iter().last() {
                    let source = source_id(&params.text_document.uri);
                    let version = i64::from(params.text_document.version);
                    match self.module_id(&params.text_document.uri) {
                        Some(module) => self.workspace.set_module_document(
                            source.clone(),
                            version,
                            change.text,
                            module,
                        ),
                        None => self
                            .workspace
                            .set_document(source.clone(), version, change.text),
                    }
                    self.open_documents.insert(source, params.text_document.uri);
                    self.publish_open_diagnostics()?;
                }
            }
            "textDocument/didClose" => {
                let params: lsp::DidCloseTextDocumentParams = params(notification.params)?;
                let source = source_id(&params.text_document.uri);
                self.open_documents.remove(&source);
                // The closed buffer is no longer authoritative, but the file
                // is still part of its package, and siblings that `use` it
                // must keep resolving. Revert to the saved file; only a
                // document with no file behind it (deleted, or outside any
                // package) leaves the workspace.
                let reverted = self
                    .module_id(&params.text_document.uri)
                    .and_then(|module| {
                        let path = uri_to_path(&params.text_document.uri)?;
                        let text = std::fs::read_to_string(&path).ok()?;
                        Some((module, text))
                    });
                match reverted {
                    Some((module, text)) => {
                        self.workspace.set_module_document(source, 0, text, module);
                    }
                    None => self.workspace.remove_document(&source),
                }
                self.send_notification(
                    "textDocument/publishDiagnostics",
                    lsp::PublishDiagnosticsParams::new(params.text_document.uri, Vec::new(), None),
                )?;
                self.publish_open_diagnostics()?;
            }
            "workspace/didChangeWatchedFiles" => {
                let params: lsp::DidChangeWatchedFilesParams = params(notification.params)?;
                self.handle_watched_files(&params.changes)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// The module name a package's manifest gives this file. A file outside any
    /// package has none, and the workspace falls back to its path.
    fn module_id(&self, uri: &lsp::Uri) -> Option<ModuleId> {
        let path = uri_to_path(uri)?;
        let package = LabPackage::discover(&path).ok()?;
        package_module_id(&package, &path)
    }

    /// Read every source module of the package holding this file, so a `use` of
    /// a sibling resolves whether or not that sibling happens to be open. A
    /// file outside a package, or one whose package fails to load, is left to
    /// the ordinary open-document path.
    fn load_package_for(&mut self, uri: &lsp::Uri) {
        let Some(path) = uri_to_path(uri) else {
            return;
        };
        let Ok(package) = LabPackage::discover(&path) else {
            return;
        };
        if !self.loaded_packages.insert(package.root.clone()) {
            return;
        }
        let documents = package
            .sources
            .iter()
            .filter_map(|source| {
                let source_id = path_to_source_id(&source.path);
                if self.workspace.contains(&source_id) {
                    return None;
                }
                let text = std::fs::read_to_string(&source.path).ok()?;
                Some((source_id, 0, text, ModuleId::new(source.module.clone())))
            })
            .collect::<Vec<_>>();
        if !documents.is_empty() {
            self.workspace.set_module_documents(documents);
        }
    }

    /// Fold file events from outside the editor — branch switches, external
    /// tools, deletions, manifest edits — back into the loaded packages. A
    /// save of an open document is not news: its buffer already holds the
    /// newest text.
    fn handle_watched_files(
        &mut self,
        changes: &[lsp::FileEvent],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut roots = BTreeSet::new();
        for change in changes {
            let Some(path) = uri_to_path(&change.uri) else {
                continue;
            };
            if change.typ == lsp::FileChangeType::CHANGED
                && self.open_documents.contains_key(&source_id(&change.uri))
            {
                continue;
            }
            if let Some(root) = self
                .loaded_packages
                .iter()
                .find(|root| path.starts_with(root))
            {
                roots.insert(root.clone());
            }
        }
        if roots.is_empty() {
            return Ok(());
        }
        for root in roots {
            self.reload_package(&root);
        }
        self.publish_open_diagnostics()
    }

    /// Re-read a loaded package from disk: seed created and externally edited
    /// files, drop files that no longer exist, and leave open documents
    /// alone — their buffers outrank the disk.
    fn reload_package(&mut self, root: &Path) {
        let Ok(package) = LabPackage::load(root) else {
            // The manifest itself is gone or invalid; forget the package so
            // the next didOpen rediscovers whatever replaces it.
            self.loaded_packages.remove(root);
            return;
        };
        let current = package
            .sources
            .iter()
            .map(|source| path_to_source_id(&source.path))
            .collect::<BTreeSet<_>>();
        let stale = self
            .workspace
            .sources()
            .into_iter()
            .filter(|source| {
                !self.open_documents.contains_key(source)
                    && !current.contains(source)
                    && source_id_to_path(source).is_some_and(|path| path.starts_with(root))
            })
            .collect::<Vec<_>>();
        for source in stale {
            self.workspace.remove_document(&source);
        }
        let documents = package
            .sources
            .iter()
            .filter_map(|source| {
                let source_id = path_to_source_id(&source.path);
                if self.open_documents.contains_key(&source_id) {
                    return None;
                }
                let text = std::fs::read_to_string(&source.path).ok()?;
                Some((source_id, 0, text, ModuleId::new(source.module.clone())))
            })
            .collect::<Vec<_>>();
        if !documents.is_empty() {
            self.workspace.set_module_documents(documents);
        }
    }
}

#[cfg(test)]
mod package_tests {
    use std::path::PathBuf;

    use lab_ide::Workspace;
    use lab_language::SourceId;

    use super::*;

    fn example(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/golden-gate")
            .join(relative)
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn opening_one_file_resolves_a_use_of_an_unopened_sibling() {
        let path = example("src/designs/plasmids.lab");
        let text = std::fs::read_to_string(&path).unwrap();
        let package = LabPackage::discover(&path).unwrap();
        let module = package_module_id(&package, &path).unwrap();

        // What the server does on didOpen: seed every package source, then set
        // the opened document under its manifest-derived name.
        let mut workspace = Workspace::new();
        workspace.set_module_documents(package.sources.iter().filter_map(|source| {
            let text = std::fs::read_to_string(&source.path).ok()?;
            Some((
                path_to_source_id(&source.path),
                0,
                text,
                ModuleId::new(source.module.clone()),
            ))
        }));
        let source = path_to_source_id(&path);
        workspace.set_module_document(source.clone(), 1, text, module);

        assert!(
            workspace.diagnostics(&source).is_empty(),
            "{:?}",
            workspace.diagnostics(&source)
        );
    }

    #[test]
    fn a_file_outside_any_package_keeps_its_synthesized_name() {
        let mut workspace = Workspace::new();
        let source = SourceId::new("file:///tmp/scratch.lab");
        workspace.set_document(
            source.clone(),
            1,
            "use std.bio.designs\n\nplasmid p:\n  sequence = dna(\"ACGT\")\n  require topology == circular\n".to_owned(),
        );

        assert!(workspace.diagnostics(&source).is_empty());
    }
}

#[cfg(test)]
mod notification_tests {
    use std::path::PathBuf;

    use lsp_server::{Connection, Message};

    use super::*;

    const DONOR: &str = "use std.bio.designs\n\nplasmid donor:\n  sequence = dna(\"ACGT\")\n  require topology == circular\n";
    const RENAMED: &str = "use std.bio.designs\n\nplasmid renamed:\n  sequence = dna(\"ACGT\")\n  require topology == circular\n";

    /// A fresh single-package fixture on disk. Each test names its own
    /// package so parallel tests never share a directory.
    fn temp_package(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("lab-language-server-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("lab.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n"),
        )
        .unwrap();
        for (relative, text) in files {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        root.canonicalize().unwrap()
    }

    fn uri(path: &Path) -> lsp::Uri {
        format!("file://{}", path.display()).parse().unwrap()
    }

    fn server() -> (Server, Connection) {
        let (server_side, client_side) = Connection::memory();
        (Server::new(server_side), client_side)
    }

    fn notify<T: serde::Serialize>(server: &mut Server, method: &str, params: T) {
        server
            .handle_notification(Notification::new(
                method.to_owned(),
                serde_json::to_value(params).unwrap(),
            ))
            .unwrap();
    }

    fn open(server: &mut Server, path: &Path) {
        let text = std::fs::read_to_string(path).unwrap();
        notify(
            server,
            "textDocument/didOpen",
            lsp::DidOpenTextDocumentParams {
                text_document: lsp::TextDocumentItem::new(uri(path), "lab".to_owned(), 1, text),
            },
        );
    }

    fn change(server: &mut Server, path: &Path, version: i32, text: &str) {
        notify(
            server,
            "textDocument/didChange",
            lsp::DidChangeTextDocumentParams {
                text_document: lsp::VersionedTextDocumentIdentifier::new(uri(path), version),
                content_changes: vec![lsp::TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: text.to_owned(),
                }],
            },
        );
    }

    fn close(server: &mut Server, path: &Path) {
        notify(
            server,
            "textDocument/didClose",
            lsp::DidCloseTextDocumentParams {
                text_document: lsp::TextDocumentIdentifier::new(uri(path)),
            },
        );
    }

    fn watched(server: &mut Server, path: &Path, typ: lsp::FileChangeType) {
        notify(
            server,
            "workspace/didChangeWatchedFiles",
            lsp::DidChangeWatchedFilesParams {
                changes: vec![lsp::FileEvent::new(uri(path), typ)],
            },
        );
    }

    fn published(client: &Connection) -> Vec<lsp::PublishDiagnosticsParams> {
        client
            .receiver
            .try_iter()
            .filter_map(|message| match message {
                Message::Notification(notification)
                    if notification.method == "textDocument/publishDiagnostics" =>
                {
                    serde_json::from_value(notification.params).ok()
                }
                _ => None,
            })
            .collect()
    }

    /// The client's current squiggles for a file: the last publish wins.
    fn latest_for(
        published: &[lsp::PublishDiagnosticsParams],
        uri: &lsp::Uri,
    ) -> Option<Vec<lsp::Diagnostic>> {
        published
            .iter()
            .rev()
            .find(|params| &params.uri == uri)
            .map(|params| params.diagnostics.clone())
    }

    #[test]
    fn closing_an_imported_files_tab_keeps_its_module_resolvable() {
        let root = temp_package(
            "closedemo",
            &[
                ("src/donor.lab", DONOR),
                ("src/main.lab", "use closedemo.donor\n\nselected = donor\n"),
            ],
        );
        let (mut server, client) = server();
        let main = root.join("src/main.lab");
        let donor = root.join("src/donor.lab");
        let main_source = path_to_source_id(&main);

        open(&mut server, &main);
        assert!(
            server.workspace.diagnostics(&main_source).is_empty(),
            "{:?}",
            server.workspace.diagnostics(&main_source)
        );

        // A preview-tab glance at the dependency: open, then close.
        open(&mut server, &donor);
        close(&mut server, &donor);

        assert!(
            server.workspace.diagnostics(&main_source).is_empty(),
            "closing a dependency's tab must not unresolve its module: {:?}",
            server.workspace.diagnostics(&main_source)
        );
        assert_eq!(
            latest_for(&published(&client), &uri(&main)).map(|d| d.is_empty()),
            Some(true),
            "the client's last word on main.lab is clean"
        );
    }

    #[test]
    fn closing_a_modified_buffer_reverts_to_the_file_on_disk() {
        let root = temp_package(
            "revertdemo",
            &[
                ("src/donor.lab", DONOR),
                ("src/main.lab", "use revertdemo.donor\n\nselected = donor\n"),
            ],
        );
        let (mut server, _client) = server();
        let main = root.join("src/main.lab");
        let donor = root.join("src/donor.lab");
        let main_source = path_to_source_id(&main);

        open(&mut server, &main);
        open(&mut server, &donor);
        change(&mut server, &donor, 2, RENAMED);
        assert!(
            !server.workspace.diagnostics(&main_source).is_empty(),
            "renaming the plasmid in the open buffer dangles main.lab's reference"
        );

        // Closing without saving discards the buffer; the saved file still
        // declares `donor`.
        close(&mut server, &donor);
        assert!(
            server.workspace.diagnostics(&main_source).is_empty(),
            "{:?}",
            server.workspace.diagnostics(&main_source)
        );
    }

    #[test]
    fn editing_a_dependency_republishes_the_importers_diagnostics() {
        let root = temp_package(
            "editdemo",
            &[
                ("src/donor.lab", DONOR),
                ("src/main.lab", "use editdemo.donor\n\nselected = donor\n"),
            ],
        );
        let (mut server, client) = server();
        let main = root.join("src/main.lab");
        let donor = root.join("src/donor.lab");

        open(&mut server, &main);
        open(&mut server, &donor);
        published(&client);

        change(&mut server, &donor, 2, RENAMED);
        assert_eq!(
            latest_for(&published(&client), &uri(&main)).map(|d| d.is_empty()),
            Some(false),
            "main.lab's dangling reference reaches the client without main.lab being touched"
        );

        change(&mut server, &donor, 3, DONOR);
        assert_eq!(
            latest_for(&published(&client), &uri(&main)).map(|d| d.is_empty()),
            Some(true),
            "restoring the declaration clears main.lab on the client again"
        );
    }

    #[test]
    fn an_external_edit_to_an_unopened_file_reanalyzes_its_importers() {
        let root = temp_package(
            "externaldemo",
            &[
                ("src/donor.lab", DONOR),
                (
                    "src/main.lab",
                    "use externaldemo.donor\n\nselected = donor\n",
                ),
            ],
        );
        let (mut server, client) = server();
        let main = root.join("src/main.lab");
        let donor = root.join("src/donor.lab");
        let main_source = path_to_source_id(&main);

        open(&mut server, &main);
        assert!(server.workspace.diagnostics(&main_source).is_empty());
        published(&client);

        // A branch switch or external tool rewrites the unopened dependency.
        std::fs::write(&donor, RENAMED).unwrap();
        watched(&mut server, &donor, lsp::FileChangeType::CHANGED);

        assert!(
            !server.workspace.diagnostics(&main_source).is_empty(),
            "the seeded copy of donor.lab must follow the disk"
        );
        assert_eq!(
            latest_for(&published(&client), &uri(&main)).map(|d| d.is_empty()),
            Some(false)
        );
    }

    #[test]
    fn a_file_created_on_disk_resolves_without_reopening_anything() {
        let root = temp_package(
            "createdemo",
            &[("src/main.lab", "use createdemo.donor\n\nselected = donor\n")],
        );
        let (mut server, _client) = server();
        let main = root.join("src/main.lab");
        let donor = root.join("src/donor.lab");
        let main_source = path_to_source_id(&main);

        open(&mut server, &main);
        assert!(
            !server.workspace.diagnostics(&main_source).is_empty(),
            "the import dangles while donor.lab does not exist"
        );

        std::fs::write(&donor, DONOR).unwrap();
        watched(&mut server, &donor, lsp::FileChangeType::CREATED);

        assert!(
            server.workspace.diagnostics(&main_source).is_empty(),
            "{:?}",
            server.workspace.diagnostics(&main_source)
        );
    }

    #[test]
    fn a_file_deleted_on_disk_stops_resolving() {
        let root = temp_package(
            "deletedemo",
            &[
                ("src/donor.lab", DONOR),
                ("src/main.lab", "use deletedemo.donor\n\nselected = donor\n"),
            ],
        );
        let (mut server, _client) = server();
        let main = root.join("src/main.lab");
        let donor = root.join("src/donor.lab");
        let main_source = path_to_source_id(&main);

        open(&mut server, &main);
        assert!(server.workspace.diagnostics(&main_source).is_empty());

        std::fs::remove_file(&donor).unwrap();
        watched(&mut server, &donor, lsp::FileChangeType::DELETED);

        assert!(
            !server.workspace.diagnostics(&main_source).is_empty(),
            "an import of a deleted module dangles"
        );
        assert!(!server.workspace.contains(&path_to_source_id(&donor)));
    }
}
