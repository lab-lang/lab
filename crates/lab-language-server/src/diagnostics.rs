//! Publishing workspace diagnostics to the client.

use std::error::Error;

use lab_language::{DiagnosticCode, DiagnosticSeverity, SourceId};
use lsp_types as lsp;

use crate::server::Server;

impl Server {
    /// Push every open document's diagnostics. An edit anywhere can change any
    /// open document's diagnostics — a `use` reaches across files — so they
    /// are republished together after each workspace change.
    pub(crate) fn publish_open_diagnostics(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        for (source, uri) in &self.open_documents {
            self.publish_diagnostics(uri, source)?;
        }
        Ok(())
    }

    fn publish_diagnostics(
        &self,
        uri: &lsp::Uri,
        source: &SourceId,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let diagnostics = self
            .workspace
            .diagnostics(source)
            .iter()
            .map(|diagnostic| lsp::Diagnostic {
                range: self.range(source, diagnostic.span),
                severity: Some(diagnostic_severity(diagnostic.severity)),
                code: Some(lsp::NumberOrString::String(
                    diagnostic_code(diagnostic.code).to_owned(),
                )),
                source: Some("lab".to_owned()),
                message: diagnostic.message.clone(),
                related_information: (!diagnostic.related.is_empty()).then(|| {
                    diagnostic
                        .related
                        .iter()
                        .filter_map(|related| {
                            Some(lsp::DiagnosticRelatedInformation {
                                location: lsp::Location {
                                    uri: related.source.0.parse().ok()?,
                                    range: self.range(&related.source, related.span),
                                },
                                message: related.message.clone(),
                            })
                        })
                        .collect()
                }),
                ..lsp::Diagnostic::default()
            })
            .collect();
        self.send_notification(
            "textDocument/publishDiagnostics",
            lsp::PublishDiagnosticsParams::new(
                uri.clone(),
                diagnostics,
                self.workspace
                    .version(source)
                    .and_then(|version| i32::try_from(version).ok()),
            ),
        )
    }
}

fn diagnostic_code(code: DiagnosticCode) -> &'static str {
    match code {
        DiagnosticCode::Syntax => "syntax",
        DiagnosticCode::Semantic => "semantic",
        DiagnosticCode::MaterialFlow => "material-flow",
    }
}

fn diagnostic_severity(severity: DiagnosticSeverity) -> lsp::DiagnosticSeverity {
    match severity {
        DiagnosticSeverity::Error => lsp::DiagnosticSeverity::ERROR,
        DiagnosticSeverity::Warning => lsp::DiagnosticSeverity::WARNING,
        DiagnosticSeverity::Information => lsp::DiagnosticSeverity::INFORMATION,
        DiagnosticSeverity::Hint => lsp::DiagnosticSeverity::HINT,
    }
}
