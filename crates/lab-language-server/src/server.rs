//! The server's state and connection loop. Requests are answered by
//! `features`, notifications are folded into the workspace by `sync`, and
//! diagnostics reach the client through `diagnostics`.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::path::PathBuf;

use lab_ide::Workspace;
use lab_language::{SourceId, Span};
use lsp_server::{Connection, Message, Notification};
use lsp_types as lsp;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::position::{offset_to_position, position_to_offset};

pub(crate) struct Server {
    pub(crate) connection: Connection,
    pub(crate) workspace: Workspace,
    /// Package roots whose source modules have already been loaded, so opening
    /// a second file in the same package does not re-read it.
    pub(crate) loaded_packages: BTreeSet<PathBuf>,
    /// Documents the editor currently has open, each with the URI it was
    /// opened under so diagnostics can be addressed back to it.
    pub(crate) open_documents: BTreeMap<SourceId, lsp::Uri>,
}

impl Server {
    pub(crate) fn new(connection: Connection) -> Self {
        Self {
            connection,
            workspace: Workspace::new(),
            loaded_packages: BTreeSet::new(),
            open_documents: BTreeMap::new(),
        }
    }

    pub(crate) fn run(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        while let Ok(message) = self.connection.receiver.recv() {
            match message {
                Message::Request(request) => {
                    if self.connection.handle_shutdown(&request)? {
                        return Ok(());
                    }
                    self.handle_request(request)?;
                }
                Message::Notification(notification) => self.handle_notification(notification)?,
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    pub(crate) fn send_notification<T: serde::Serialize>(
        &self,
        method: &str,
        params: T,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.connection
            .sender
            .send(Message::Notification(Notification::new(
                method.to_owned(),
                params,
            )))?;
        Ok(())
    }

    pub(crate) fn offset(&self, source: &SourceId, position: lsp::Position) -> usize {
        position_to_offset(self.workspace.text(source).unwrap_or_default(), position)
    }

    pub(crate) fn position(&self, source: &SourceId, offset: usize) -> lsp::Position {
        offset_to_position(self.workspace.text(source).unwrap_or_default(), offset)
    }

    pub(crate) fn range(&self, source: &SourceId, span: Span) -> lsp::Range {
        lsp::Range::new(
            self.position(source, span.start),
            self.position(source, span.end),
        )
    }
}

pub(crate) fn params<T: DeserializeOwned>(value: Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(value)
}
