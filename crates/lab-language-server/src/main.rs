use std::collections::BTreeMap;
use std::error::Error;

use lab_ide::{SemanticTokenKind, SymbolKind, Workspace};
use lab_language::{DiagnosticCode, DiagnosticSeverity, SourceId, Span};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types as lsp;
use serde::de::DeserializeOwned;
use serde_json::Value;

const TOKEN_TYPES: &[lsp::SemanticTokenType] = &[
    lsp::SemanticTokenType::COMMENT,
    lsp::SemanticTokenType::KEYWORD,
    lsp::SemanticTokenType::STRING,
    lsp::SemanticTokenType::NUMBER,
    lsp::SemanticTokenType::TYPE,
    lsp::SemanticTokenType::FUNCTION,
    lsp::SemanticTokenType::VARIABLE,
    lsp::SemanticTokenType::OPERATOR,
];

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
    let capabilities = serde_json::to_value(server_capabilities())?;
    let initialization = connection.initialize(capabilities)?;
    let _: lsp::InitializeParams = serde_json::from_value(initialization)?;

    let mut server = Server {
        connection,
        workspace: Workspace::new(),
    };
    server.run()?;
    drop(server);
    io_threads.join()?;
    Ok(())
}

fn server_capabilities() -> lsp::ServerCapabilities {
    lsp::ServerCapabilities {
        text_document_sync: Some(lsp::TextDocumentSyncCapability::Kind(
            lsp::TextDocumentSyncKind::FULL,
        )),
        completion_provider: Some(lsp::CompletionOptions::default()),
        hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
        definition_provider: Some(lsp::OneOf::Left(true)),
        references_provider: Some(lsp::OneOf::Left(true)),
        rename_provider: Some(lsp::OneOf::Left(true)),
        document_symbol_provider: Some(lsp::OneOf::Left(true)),
        document_formatting_provider: Some(lsp::OneOf::Left(true)),
        semantic_tokens_provider: Some(
            lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                lsp::SemanticTokensOptions {
                    legend: lsp::SemanticTokensLegend {
                        token_types: TOKEN_TYPES.to_vec(),
                        token_modifiers: Vec::new(),
                    },
                    range: None,
                    full: Some(lsp::SemanticTokensFullOptions::Bool(true)),
                    ..lsp::SemanticTokensOptions::default()
                },
            ),
        ),
        ..lsp::ServerCapabilities::default()
    }
}

struct Server {
    connection: Connection,
    workspace: Workspace,
}

impl Server {
    fn run(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
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

    fn handle_notification(
        &mut self,
        notification: Notification,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                let params: lsp::DidOpenTextDocumentParams = params(notification.params)?;
                let source = source_id(&params.text_document.uri);
                self.workspace.set_document(
                    source.clone(),
                    i64::from(params.text_document.version),
                    params.text_document.text,
                );
                self.publish_diagnostics(&params.text_document.uri, &source)?;
            }
            "textDocument/didChange" => {
                let params: lsp::DidChangeTextDocumentParams = params(notification.params)?;
                if let Some(change) = params.content_changes.into_iter().last() {
                    let source = source_id(&params.text_document.uri);
                    self.workspace.set_document(
                        source.clone(),
                        i64::from(params.text_document.version),
                        change.text,
                    );
                    self.publish_diagnostics(&params.text_document.uri, &source)?;
                }
            }
            "textDocument/didClose" => {
                let params: lsp::DidCloseTextDocumentParams = params(notification.params)?;
                let source = source_id(&params.text_document.uri);
                self.workspace.remove_document(&source);
                self.send_notification(
                    "textDocument/publishDiagnostics",
                    lsp::PublishDiagnosticsParams::new(params.text_document.uri, Vec::new(), None),
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_request(&self, request: Request) -> Result<(), Box<dyn Error + Send + Sync>> {
        let response = match request.method.as_str() {
            "textDocument/completion" => self.completion(&request)?,
            "textDocument/hover" => self.hover(&request)?,
            "textDocument/definition" => self.definition(&request)?,
            "textDocument/references" => self.references(&request)?,
            "textDocument/rename" => self.rename(&request)?,
            "textDocument/documentSymbol" => self.document_symbols(&request)?,
            "textDocument/semanticTokens/full" => self.semantic_tokens(&request)?,
            "textDocument/formatting" => self.formatting(&request)?,
            _ => Response::new_err(
                request.id,
                lsp_server::ErrorCode::MethodNotFound as i32,
                format!("unsupported method '{}'", request.method),
            ),
        };
        self.connection.sender.send(Message::Response(response))?;
        Ok(())
    }

    fn completion(&self, request: &Request) -> Result<Response, serde_json::Error> {
        let params: lsp::CompletionParams = params(request.params.clone())?;
        let source = source_id(&params.text_document_position.text_document.uri);
        let offset = self.offset(&source, params.text_document_position.position);
        let items = self
            .workspace
            .completions(&source, offset)
            .into_iter()
            .map(|item| lsp::CompletionItem {
                label: item.label,
                kind: Some(match item.kind {
                    lab_ide::CompletionKind::Keyword => lsp::CompletionItemKind::KEYWORD,
                    lab_ide::CompletionKind::Type => lsp::CompletionItemKind::CLASS,
                    lab_ide::CompletionKind::Value => lsp::CompletionItemKind::VARIABLE,
                    lab_ide::CompletionKind::Function => lsp::CompletionItemKind::FUNCTION,
                    lab_ide::CompletionKind::Module => lsp::CompletionItemKind::MODULE,
                }),
                detail: item.detail,
                ..lsp::CompletionItem::default()
            })
            .collect::<Vec<_>>();
        ok(request, lsp::CompletionResponse::Array(items))
    }

    fn hover(&self, request: &Request) -> Result<Response, serde_json::Error> {
        let params: lsp::HoverParams = params(request.params.clone())?;
        let source = source_id(&params.text_document_position_params.text_document.uri);
        let offset = self.offset(&source, params.text_document_position_params.position);
        let result = self
            .workspace
            .hover(&source, offset)
            .map(|hover| lsp::Hover {
                contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                    kind: lsp::MarkupKind::Markdown,
                    value: hover.markdown,
                }),
                range: Some(self.range(&source, hover.span)),
            });
        ok(request, result)
    }

    fn definition(&self, request: &Request) -> Result<Response, serde_json::Error> {
        let params: lsp::GotoDefinitionParams = params(request.params.clone())?;
        let source = source_id(&params.text_document_position_params.text_document.uri);
        let offset = self.offset(&source, params.text_document_position_params.position);
        let result = self
            .workspace
            .definition(&source, offset)
            .and_then(|location| {
                Some(lsp::GotoDefinitionResponse::Scalar(lsp::Location {
                    uri: location.source.0.parse().ok()?,
                    range: self.range(&location.source, location.span),
                }))
            });
        ok(request, result)
    }

    fn references(&self, request: &Request) -> Result<Response, serde_json::Error> {
        let params: lsp::ReferenceParams = params(request.params.clone())?;
        let source = source_id(&params.text_document_position.text_document.uri);
        let offset = self.offset(&source, params.text_document_position.position);
        let declaration = (!params.context.include_declaration)
            .then(|| self.workspace.definition(&source, offset))
            .flatten();
        let locations = self
            .workspace
            .references(&source, offset)
            .into_iter()
            .filter(|location| declaration.as_ref() != Some(location))
            .filter_map(|location| {
                Some(lsp::Location {
                    uri: location.source.0.parse().ok()?,
                    range: self.range(&location.source, location.span),
                })
            })
            .collect::<Vec<_>>();
        ok(request, locations)
    }

    fn rename(&self, request: &Request) -> Result<Response, serde_json::Error> {
        let params: lsp::RenameParams = params(request.params.clone())?;
        let source = source_id(&params.text_document_position.text_document.uri);
        let offset = self.offset(&source, params.text_document_position.position);
        let mut changes = BTreeMap::<String, Vec<lsp::TextEdit>>::new();
        for edit in self.workspace.rename(&source, offset, &params.new_name) {
            changes
                .entry(edit.source.0.clone())
                .or_default()
                .push(lsp::TextEdit {
                    range: self.range(&edit.source, edit.span),
                    new_text: edit.new_text,
                });
        }
        let edits = changes
            .into_iter()
            .filter_map(|(uri, edits)| {
                Some(lsp::TextDocumentEdit {
                    text_document: lsp::OptionalVersionedTextDocumentIdentifier {
                        uri: uri.parse().ok()?,
                        version: None,
                    },
                    edits: edits.into_iter().map(lsp::OneOf::Left).collect(),
                })
            })
            .collect();
        ok(
            request,
            lsp::WorkspaceEdit {
                changes: None,
                document_changes: Some(lsp::DocumentChanges::Edits(edits)),
                change_annotations: None,
            },
        )
    }

    #[allow(deprecated)]
    fn document_symbols(&self, request: &Request) -> Result<Response, serde_json::Error> {
        let params: lsp::DocumentSymbolParams = params(request.params.clone())?;
        let source = source_id(&params.text_document.uri);
        let symbols = self
            .workspace
            .document_symbols(&source)
            .into_iter()
            .map(|symbol| self.document_symbol(&source, symbol))
            .collect::<Vec<_>>();
        ok(request, lsp::DocumentSymbolResponse::Nested(symbols))
    }

    #[allow(deprecated)]
    fn document_symbol(
        &self,
        source: &SourceId,
        symbol: lab_ide::DocumentSymbol,
    ) -> lsp::DocumentSymbol {
        lsp::DocumentSymbol {
            name: symbol.name,
            detail: None,
            kind: symbol_kind(symbol.kind),
            tags: None,
            deprecated: None,
            range: self.range(source, symbol.span),
            selection_range: self.range(source, symbol.selection_span),
            children: (!symbol.children.is_empty()).then(|| {
                symbol
                    .children
                    .into_iter()
                    .map(|child| self.document_symbol(source, child))
                    .collect()
            }),
        }
    }

    fn semantic_tokens(&self, request: &Request) -> Result<Response, serde_json::Error> {
        let params: lsp::SemanticTokensParams = params(request.params.clone())?;
        let source = source_id(&params.text_document.uri);
        let mut previous_line = 0;
        let mut previous_start = 0;
        let data = self
            .workspace
            .semantic_tokens(&source)
            .into_iter()
            .map(|token| {
                let start = self.position(&source, token.span.start);
                let end = self.position(&source, token.span.end);
                let delta_line = start.line - previous_line;
                let delta_start = if delta_line == 0 {
                    start.character - previous_start
                } else {
                    start.character
                };
                previous_line = start.line;
                previous_start = start.character;
                lsp::SemanticToken {
                    delta_line,
                    delta_start,
                    length: end.character.saturating_sub(start.character),
                    token_type: semantic_token_index(token.kind),
                    token_modifiers_bitset: 0,
                }
            })
            .collect();
        ok(
            request,
            lsp::SemanticTokensResult::Tokens(lsp::SemanticTokens {
                result_id: self
                    .workspace
                    .version(&source)
                    .map(|version| version.to_string()),
                data,
            }),
        )
    }

    fn formatting(&self, request: &Request) -> Result<Response, serde_json::Error> {
        let params: lsp::DocumentFormattingParams = params(request.params.clone())?;
        let source = source_id(&params.text_document.uri);
        let edits = self
            .workspace
            .format_document(&source)
            .map_or_else(Vec::new, |new_text| {
                vec![lsp::TextEdit {
                    range: self.range(
                        &source,
                        Span::new(0, self.workspace.text(&source).map_or(0, str::len)),
                    ),
                    new_text,
                }]
            });
        ok(request, edits)
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

    fn send_notification<T: serde::Serialize>(
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

    fn offset(&self, source: &SourceId, position: lsp::Position) -> usize {
        position_to_offset(self.workspace.text(source).unwrap_or_default(), position)
    }

    fn position(&self, source: &SourceId, offset: usize) -> lsp::Position {
        offset_to_position(self.workspace.text(source).unwrap_or_default(), offset)
    }

    fn range(&self, source: &SourceId, span: Span) -> lsp::Range {
        lsp::Range::new(
            self.position(source, span.start),
            self.position(source, span.end),
        )
    }
}

fn params<T: DeserializeOwned>(value: Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(value)
}

fn ok<T: serde::Serialize>(request: &Request, result: T) -> Result<Response, serde_json::Error> {
    Ok(Response::new_ok(
        request.id.clone(),
        serde_json::to_value(result)?,
    ))
}

fn source_id(uri: &lsp::Uri) -> SourceId {
    SourceId::new(uri.as_str())
}

fn offset_to_position(text: &str, requested: usize) -> lsp::Position {
    let offset = requested.min(text.len());
    let prefix = &text[..text.floor_char_boundary(offset)];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let character = prefix[line_start..].encode_utf16().count() as u32;
    lsp::Position::new(line, character)
}

fn position_to_offset(text: &str, position: lsp::Position) -> usize {
    let mut offset = 0;
    let mut lines = text.split_inclusive('\n');
    for _ in 0..position.line {
        offset += lines.next().map_or(0, str::len);
    }
    let line = lines.next().unwrap_or_default();
    let mut utf16 = 0;
    for (byte, character) in line.char_indices() {
        if utf16 >= position.character {
            return offset + byte;
        }
        utf16 += character.len_utf16() as u32;
    }
    offset + line.trim_end_matches('\n').len()
}

fn symbol_kind(kind: SymbolKind) -> lsp::SymbolKind {
    match kind {
        SymbolKind::Module => lsp::SymbolKind::MODULE,
        SymbolKind::Circuit | SymbolKind::Workflow => lsp::SymbolKind::FUNCTION,
        SymbolKind::Plasmid | SymbolKind::Data => lsp::SymbolKind::STRUCT,
        SymbolKind::Variable => lsp::SymbolKind::VARIABLE,
        SymbolKind::Field => lsp::SymbolKind::FIELD,
        SymbolKind::Case => lsp::SymbolKind::ENUM_MEMBER,
    }
}

fn semantic_token_index(kind: SemanticTokenKind) -> u32 {
    match kind {
        SemanticTokenKind::Comment => 0,
        SemanticTokenKind::Keyword => 1,
        SemanticTokenKind::String => 2,
        SemanticTokenKind::Number => 3,
        SemanticTokenKind::Type => 4,
        SemanticTokenKind::Function => 5,
        SemanticTokenKind::Variable => 6,
        SemanticTokenKind::Operator => 7,
    }
}

fn diagnostic_code(code: DiagnosticCode) -> &'static str {
    match code {
        DiagnosticCode::Syntax => "syntax",
        DiagnosticCode::Unsupported => "unsupported",
        DiagnosticCode::Specification => "specification",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf16_positions() {
        let text = "a😀b\nnext";
        assert_eq!(offset_to_position(text, 5), lsp::Position::new(0, 3));
        assert_eq!(position_to_offset(text, lsp::Position::new(0, 3)), 5);
        assert_eq!(position_to_offset(text, lsp::Position::new(1, 2)), 9);
    }
}
