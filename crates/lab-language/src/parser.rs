use crate::ast::*;
use crate::error::{ParseError, syntax_span};
use crate::lexer::lex;
use crate::source::{Identifier, Span, Spanned};
use crate::token::{Token, TokenKind};

/// The word instances of a type are written with: the type's own name, in
/// snake_case. `Plasmid` gives `plasmid`, `RestrictionEnzyme` gives
/// `restriction_enzyme`.
fn instance_word(produces: &TypeExpr) -> Result<String, ParseError> {
    let TypeExpr::Path { path, span, .. } = produces else {
        return Err(syntax_span(
            produces.span(),
            "an artifact kind names a type its instances have",
        ));
    };
    let [segment] = path.segments.as_slice() else {
        return Err(syntax_span(
            *span,
            "an artifact kind names a type declared here or imported, not a path",
        ));
    };
    // A break belongs where a word does: after a lowercase run, or at the end
    // of an acronym. `RestrictionEnzyme` gives `restriction_enzyme` and `DNA`
    // gives `dna` rather than `d_n_a`.
    let characters = segment.value.chars().collect::<Vec<_>>();
    let mut word = String::new();
    for (index, character) in characters.iter().enumerate() {
        let previous = index.checked_sub(1).map(|index| characters[index]);
        let next = characters.get(index + 1).copied();
        let opens_word = previous.is_some_and(|previous| !previous.is_uppercase());
        let ends_acronym = previous.is_some_and(char::is_uppercase)
            && next.is_some_and(|next| next.is_lowercase());
        if character.is_uppercase() && (opens_word || ends_acronym) {
            word.push('_');
        }
        word.extend(character.to_lowercase());
    }
    Ok(word)
}

/// Parse a complete Lab source module without lowering it.
pub fn parse_module(source: &str) -> Result<Module, ParseError> {
    Parser::new(source, lex(source)?).parse_module()
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    cursor: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            cursor: 0,
        }
    }

    fn parse_module(mut self) -> Result<Module, ParseError> {
        let mut items = Vec::new();
        self.skip_newlines();
        let doc = self.take_module_doc()?;
        while !self.at_end() {
            let doc = self.take_doc()?;
            let item = if self.check_word("use") {
                if doc.is_some() {
                    return Err(syntax_span(
                        self.current_span(),
                        "documentation describes a declaration, not an import; use '//' to comment on an import",
                    ));
                }
                Item::Use(self.parse_use()?)
            } else if self.check_word("role") {
                let mut declaration = self.parse_role()?;
                declaration.doc = doc;
                Item::Role(declaration)
            } else if self.check_word("circuit") {
                let mut declaration = self.parse_circuit()?;
                declaration.doc = doc;
                Item::Circuit(declaration)
            } else if self.check_word("artifact") {
                let mut declaration = self.parse_artifact_kind()?;
                declaration.doc = doc;
                Item::ArtifactKind(declaration)
            } else if self.check_word("record") {
                let mut declaration = self.parse_data()?;
                declaration.doc = doc;
                Item::Data(declaration)
            } else if self.check_word("workflow") {
                let mut declaration = self.parse_workflow()?;
                declaration.doc = doc;
                Item::Workflow(declaration)
            } else if self.opens_provenance_block() {
                // A provenance verb followed by a block states one origin over
                // everything inside, so a program reads as its inventory and
                // its recipes rather than as a verb repeated per line.
                if doc.is_some() {
                    return Err(syntax_span(
                        self.current_span(),
                        "documentation describes one declaration; document each thing inside the block",
                    ));
                }
                for declaration in self.parse_provenance_block()? {
                    items.push(Item::Artifact(declaration));
                }
                self.skip_newlines();
                continue;
            } else if self.opens_artifact() {
                // A word this parser has never heard of, followed by a name and
                // a block, is an artifact instance. Which kind it names is a
                // question for the checker, so the grammar stays closed while
                // the vocabulary stays open.
                let mut declaration = self.parse_artifact()?;
                declaration.doc = doc;
                Item::Artifact(declaration)
            } else {
                let mut declaration = self.parse_binding()?;
                declaration.doc = doc;
                Item::Binding(declaration)
            };
            items.push(item);
            self.skip_newlines();
        }
        Ok(Module {
            doc,
            items,
            span: Span::new(0, self.source.len()),
        })
    }

    fn parse_use(&mut self) -> Result<UseDecl, ParseError> {
        let start = self.expect_word("use")?.span;
        let path = self.parse_path()?;
        let end = self.expect_line_end()?;
        Ok(UseDecl {
            span: start.join(end),
            path,
        })
    }

    /// `role Signal` — a name types can play. A role has no block: its members
    /// are declared by the types that play it, so a package can add members to
    /// a role it imports.
    fn parse_role(&mut self) -> Result<RoleDecl, ParseError> {
        let start = self.expect_word("role")?.span;
        let name = self.take_identifier("a role name")?;
        let end = self.expect_line_end()?;
        Ok(RoleDecl {
            doc: None,
            name,
            span: start.join(end),
        })
    }

    /// The `is Signal, Reporter` clause on a declaration that plays roles.
    fn parse_roles_clause(&mut self) -> Result<Vec<Path>, ParseError> {
        if !self.check_word("is") {
            return Ok(Vec::new());
        }
        self.next();
        let mut roles = vec![self.parse_path()?];
        while self.consume(&TokenKind::Comma).is_some() {
            roles.push(self.parse_path()?);
        }
        Ok(roles)
    }

    /// A circuit is called, so it declares a callable signature exactly as a
    /// workflow does. Its block holds only the parts it is built from.
    fn parse_circuit(&mut self) -> Result<CircuitDecl, ParseError> {
        let start = self.expect_word("circuit")?.span;
        let name = self.take_identifier("a circuit name")?;
        let inputs = self.parse_signature_fields("a parameter name")?;
        self.expect(TokenKind::RightArrow)?;
        let output = self.parse_type()?;
        self.open_block()?;

        let mut sections = Vec::new();
        while !self.check(&TokenKind::Dedent) {
            sections.push(self.parse_section()?);
        }
        let end = self.expect(TokenKind::Dedent)?.span;
        Ok(CircuitDecl {
            doc: None,
            name,
            inputs,
            output,
            sections,
            span: start.join(end),
        })
    }

    /// Whether the next item is `word Name:` — the shape every artifact
    /// instance has, whichever package supplied the word.
    fn opens_artifact(&self) -> bool {
        // A verb says outright that this declares a thing, so nothing further
        // is needed to tell it apart. Without one, the colon is what separates
        // `plasmid p_gfp:` from an ordinary binding.
        if self.opens_provenance() {
            return matches!(self.peek_kind(1), Some(TokenKind::Identifier(_)))
                && matches!(self.peek_kind(2), Some(TokenKind::Identifier(_)));
        }
        matches!(self.peek_kind(0), Some(TokenKind::Identifier(_)))
            && matches!(self.peek_kind(1), Some(TokenKind::Identifier(_)))
            && self.peek_kind(2) == Some(&TokenKind::Colon)
    }

    /// Whether the next word states where a thing came from.
    fn opens_provenance(&self) -> bool {
        self.check_word("build") || self.check_word("buy")
    }

    /// Whether the next item is `buy:` or `build:` — a provenance verb whose
    /// block states where everything inside it came from.
    fn opens_provenance_block(&self) -> bool {
        self.opens_provenance() && self.peek_kind(1) == Some(&TokenKind::Colon)
    }

    /// `buy:` — one provenance over a block of instances. Each line inside is
    /// an instance without a verb, and each lowers to its own declaration; the
    /// block is surface grouping, not a node.
    fn parse_provenance_block(&mut self) -> Result<Vec<ArtifactDecl>, ParseError> {
        let verb = self.next().expect("checked");
        let provenance = match &verb.kind {
            TokenKind::Identifier(word) if word == "buy" => Provenance::Buy,
            _ => Provenance::Build,
        };
        self.open_block()?;
        let mut declarations = Vec::new();
        while !self.check(&TokenKind::Dedent) {
            let doc = self.take_doc()?;
            if self.opens_provenance() {
                return Err(syntax_span(
                    self.current_span(),
                    "the block already states where everything in it came from; write the thing without a verb",
                ));
            }
            // Each declaration's span is its own lines: a diagnostic about one
            // instance has no business underlining the whole block.
            let mut declaration = self.parse_artifact_instance(provenance, None)?;
            declaration.doc = doc;
            declarations.push(declaration);
        }
        self.expect(TokenKind::Dedent)?;
        Ok(declarations)
    }

    /// `artifact plasmid:` — the schema a package declares for its own kind.
    fn parse_artifact_kind(&mut self) -> Result<ArtifactKindDecl, ParseError> {
        let start = self.expect_word("artifact")?.span;
        // A kind names the type its instances have. The word those instances
        // are written with is that type in snake_case, so neither is written
        // twice and the two can never disagree.
        let produces = self.parse_type()?;
        let name = Identifier::new(instance_word(&produces)?, produces.span());
        // A kind whose instances state nothing beyond their name needs no
        // block, the way a role needs none.
        if !self.check(&TokenKind::Colon) {
            let end = self.expect_line_end()?;
            return Ok(ArtifactKindDecl {
                doc: None,
                name,
                produces,
                fields: Vec::new(),
                declares: None,
                span: start.join(end),
            });
        }
        self.open_block()?;
        let mut fields = Vec::new();
        let mut declares = None;
        while !self.check(&TokenKind::Dedent) {
            if self.check_word("declares") {
                let keyword = self.next().expect("checked");
                if declares.is_some() {
                    return Err(syntax_span(
                        keyword.span,
                        "a kind states which combinations are complete once",
                    ));
                }
                declares = Some(self.parse_expr()?);
                self.expect_line_end()?;
            } else {
                fields.push(self.parse_field_line(true)?);
            }
        }
        let end = self.expect(TokenKind::Dedent)?.span;
        Ok(ArtifactKindDecl {
            doc: None,
            name,
            produces,
            fields,
            declares,
            span: start.join(end),
        })
    }

    /// `across 3 biological replicates` — the evidence a claim is believed on.
    ///
    /// The word "biological" is written out because the distinction it draws is
    /// the whole point: measuring one colony three times is not three
    /// replicates, and a reader should not have to guess which is meant.
    fn parse_replication(&mut self) -> Result<Replication, ParseError> {
        let start = self.expect_word("across")?.span;
        let token = self
            .next()
            .ok_or_else(|| syntax_span(self.current_span(), "expected a number of replicates"))?;
        let TokenKind::Integer(count) = token.kind else {
            return Err(syntax_span(token.span, "expected a number of replicates"));
        };
        self.expect_word("biological")?;
        if !self.check_word("replicates") && !self.check_word("replicate") {
            return Err(syntax_span(self.current_span(), "expected 'replicates'"));
        }
        let end = self.next().expect("checked").span;
        Ok(Replication {
            count,
            span: start.join(end),
        })
    }

    fn parse_artifact(&mut self) -> Result<ArtifactDecl, ParseError> {
        let verb = self
            .opens_provenance()
            .then(|| self.next().expect("checked"));
        let provenance = match verb.as_ref().map(|token| &token.kind) {
            Some(TokenKind::Identifier(word)) if word == "buy" => Provenance::Buy,
            _ => Provenance::Build,
        };
        self.parse_artifact_instance(provenance, verb.map(|token| token.span))
    }

    /// The instance itself — everything after the verb, which a standalone
    /// declaration writes inline and a provenance block writes once above.
    fn parse_artifact_instance(
        &mut self,
        provenance: Provenance,
        verb: Option<Span>,
    ) -> Result<ArtifactDecl, ParseError> {
        let kind = self.take_identifier("an artifact kind")?;
        let start = verb.unwrap_or(kind.span);
        let name = self.take_identifier("an artifact name")?;
        // A generic kind cannot say from its word alone which arguments an
        // instance has, so the instance names its own type. A word whose kind
        // takes no arguments already said it, and repeating it says nothing.
        // As everywhere else, a ':' ending the header opens a block:
        // `buy enzyme BsaI:` opens one directly, and an ascribed instance
        // opens one after its type, the way a workflow does after its result.
        let mut opens_block = false;
        let ascribed = if self.consume(&TokenKind::Colon).is_some() {
            if self.check(&TokenKind::Newline) {
                opens_block = true;
                None
            } else {
                let ascribed = self.parse_type()?;
                opens_block = self.consume(&TokenKind::Colon).is_some();
                Some(ascribed)
            }
        } else {
            None
        };
        // A block is optional: an item that states nothing about itself is a
        // name and a kind, and that is the common case for something bought.
        let mut end = self.expect_line_end()?;
        let mut members = Vec::new();
        if !opens_block {
            if self.check(&TokenKind::Indent) {
                return Err(syntax_span(
                    self.current_span(),
                    "a declaration block is opened by ':' at the end of the line above",
                ));
            }
            return Ok(ArtifactDecl {
                doc: None,
                provenance,
                kind,
                name,
                ascribed,
                members,
                span: start.join(end),
            });
        }
        self.expect(TokenKind::Indent)?;
        while !self.check(&TokenKind::Dedent) {
            if self.check_word("across") {
                let replication = self.parse_replication()?;
                self.expect_line_end()?;
                members.push(ArtifactMember::Replication(replication));
            } else if self.check_word("require") || self.check_word("accept") {
                let acceptance = self.check_word("accept");
                let keyword = self.next().expect("checked");
                let predicate = self.parse_expr()?;
                // A claim may state its own standard, which is what it is
                // believed on rather than what the declaration asks for.
                let replicates = if self.check_word("across") {
                    Some(self.parse_replication()?)
                } else {
                    None
                };
                let end = self.expect_line_end()?;
                let claim = ClaimStmt {
                    predicate,
                    replicates,
                    span: keyword.span.join(end),
                };
                members.push(if acceptance {
                    ArtifactMember::Acceptance(claim)
                } else {
                    ArtifactMember::Requirement(claim)
                });
            } else if self.peek_kind(1) == Some(&TokenKind::Equal) {
                members.push(ArtifactMember::Property(self.parse_property()?));
            } else {
                members.push(ArtifactMember::Section(self.parse_section()?));
            }
        }
        end = self.expect(TokenKind::Dedent)?.span;
        Ok(ArtifactDecl {
            doc: None,
            provenance,
            kind,
            name,
            ascribed,
            members,
            span: start.join(end),
        })
    }

    fn parse_data(&mut self) -> Result<DataDecl, ParseError> {
        let keyword = self.expect_word("record")?;
        let name = self.take_identifier("a declaration name")?;
        let parameters = self.parse_type_parameters()?;
        let roles = self.parse_roles_clause()?;
        // A declaration with no fields carries no block. A tag whose whole
        // content is its identity — `record Tetracycline is Signal` — is a
        // complete declaration, not a truncated one.
        if !self.check(&TokenKind::Colon) {
            let end = self.expect_line_end()?;
            return Ok(DataDecl {
                doc: None,
                name,
                parameters,
                roles,
                fields: Vec::new(),
                cases: Vec::new(),
                span: keyword.span.join(end),
            });
        }
        self.open_block()?;
        let mut fields = Vec::new();
        let mut cases = Vec::new();
        while !self.check(&TokenKind::Dedent) {
            if self.check_word("case") {
                cases.push(self.parse_case_decl()?);
            } else {
                fields.push(self.parse_field_line(false)?);
            }
        }
        let end = self.expect(TokenKind::Dedent)?.span;
        Ok(DataDecl {
            doc: None,
            name,
            parameters,
            roles,
            fields,
            cases,
            span: keyword.span.join(end),
        })
    }

    fn parse_case_decl(&mut self) -> Result<CaseDecl, ParseError> {
        let start = self.expect_word("case")?.span;
        let name = self.take_identifier("a case name")?;
        let mut fields = Vec::new();
        let end = if self.consume(&TokenKind::Colon).is_some() {
            self.expect(TokenKind::Newline)?;
            self.expect(TokenKind::Indent)?;
            while !self.check(&TokenKind::Dedent) {
                fields.push(self.parse_field_line(false)?);
            }
            self.expect(TokenKind::Dedent)?.span
        } else {
            self.expect_line_end()?
        };
        Ok(CaseDecl {
            name,
            fields,
            span: start.join(end),
        })
    }

    fn parse_workflow(&mut self) -> Result<WorkflowDecl, ParseError> {
        let start = self.expect_word("workflow")?.span;
        let name = self.take_identifier("a workflow name")?;
        let inputs = self.parse_signature_fields("a parameter name")?;
        self.expect(TokenKind::RightArrow)?;
        let outputs = if self.check(&TokenKind::LeftParen) {
            let fields = self.parse_signature_fields("a result name")?;
            if fields.is_empty() {
                return Err(syntax_span(
                    name.span,
                    "a named workflow result list cannot be empty; use 'None' for no value",
                ));
            }
            WorkflowOutputs::Named { fields }
        } else {
            WorkflowOutputs::Single {
                ty: self.parse_type()?,
            }
        };
        self.open_block()?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::Dedent) {
            body.push(self.parse_stmt()?);
        }
        let end = self.expect(TokenKind::Dedent)?.span;
        Ok(WorkflowDecl {
            doc: None,
            name,
            inputs,
            outputs,
            body,
            span: start.join(end),
        })
    }

    fn parse_signature_fields(
        &mut self,
        expected_name: &'static str,
    ) -> Result<Vec<FieldDecl>, ParseError> {
        self.expect(TokenKind::LeftParen)?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RightParen) {
            let name = self.take_identifier(expected_name)?;
            if let Some(token) = self.consume(&TokenKind::Question) {
                return Err(syntax_span(
                    token.span,
                    "every caller supplies every parameter, so a parameter is never optional",
                ));
            }
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type()?;
            let span = name.span.join(ty.span());
            fields.push(FieldDecl {
                name,
                ty,
                optional: false,
                span,
            });
            if self.consume(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(TokenKind::RightParen)?;
        Ok(fields)
    }

    fn parse_section(&mut self) -> Result<Section, ParseError> {
        let name = self.take_identifier("a section name")?;
        self.expect(TokenKind::Colon)?;
        self.expect(TokenKind::Newline)?;
        self.expect(TokenKind::Indent)?;
        let mut entries = Vec::new();
        while !self.check(&TokenKind::Dedent) {
            entries.push(self.parse_expr()?);
            self.expect_line_end()?;
        }
        let end = self.expect(TokenKind::Dedent)?.span;
        Ok(Section {
            span: name.span.join(end),
            name,
            entries,
        })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        if self.check_word("state") {
            return self.parse_state().map(Stmt::State);
        }
        if self.check_word("return") {
            return self.parse_return().map(Stmt::Return);
        }
        if self.check_word("if") {
            return self.parse_if().map(Stmt::If);
        }
        if self.check_word("match") {
            return self.parse_match().map(Stmt::Match);
        }
        if self.check_word("for") {
            return self.parse_for().map(Stmt::For);
        }
        if self.check_word("when") {
            return self.parse_when().map(Stmt::When);
        }
        if self.check_word("emit") {
            return self.parse_emit().map(Stmt::Emit);
        }
        if self.check(&TokenKind::LeftArrow) || self.line_has(TokenKind::LeftArrow) {
            return self.parse_effect().map(Stmt::Effect);
        }
        self.parse_binding().map(Stmt::Binding)
    }

    fn parse_state(&mut self) -> Result<StateStmt, ParseError> {
        let start = self.expect_word("state")?.span;
        let name = self.take_identifier("a state name")?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        self.expect(TokenKind::Equal)?;
        let initial = self.parse_expr()?;
        let end = self.expect_line_end()?;
        Ok(StateStmt {
            name,
            ty,
            initial,
            span: start.join(end),
        })
    }

    fn parse_binding(&mut self) -> Result<BindingStmt, ParseError> {
        let name = self.take_identifier("a binding name")?;
        let start = name.span;
        let annotation = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expr()?;
        let end = self.expect_line_end()?;
        Ok(BindingStmt {
            doc: None,
            names: vec![name],
            annotation,
            value,
            span: start.join(end),
        })
    }

    /// `name = value` inside a declaration.
    ///
    /// A property associates a name with a value, and `:` is reserved for the
    /// other thing a declaration body can say — that a name has a type. One
    /// token after the name decides which, with no need to know the word that
    /// opened the block.
    fn parse_property(&mut self) -> Result<PropertyDecl, ParseError> {
        let name = self.take_identifier("a property name")?;
        let start = name.span;
        self.expect(TokenKind::Equal)?;
        let value = self.parse_expr()?;
        let end = self.expect_line_end()?;
        Ok(PropertyDecl {
            name,
            value,
            span: start.join(end),
        })
    }

    fn parse_effect(&mut self) -> Result<EffectStmt, ParseError> {
        let start = self.current_span();
        let mut names = Vec::new();
        if !self.check(&TokenKind::LeftArrow) {
            loop {
                names.push(self.take_identifier("an effect result name")?);
                if self.consume(&TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(TokenKind::LeftArrow)?;
        let action_start = self.current_span().start;
        let mut action_end = action_start;
        while !self.at_end() && !self.check(&TokenKind::Newline) {
            action_end = self.next().expect("not at end").span.end;
        }
        if action_start == action_end {
            return Err(syntax_span(
                start,
                "an effect requires an action after '<-'",
            ));
        }
        let end = self.expect_line_end()?;
        // The phrase keeps its place in the source so a diagnostic can point at
        // one operand rather than at the whole line.
        let raw = &self.source[action_start..action_end];
        let leading = raw.len() - raw.trim_start().len();
        let action = raw.trim();
        let phrase = Span::new(
            action_start + leading,
            action_start + leading + action.len(),
        );
        Ok(EffectStmt {
            names,
            action: action.to_owned(),
            phrase,
            span: start.join(end),
        })
    }

    fn parse_return(&mut self) -> Result<ReturnStmt, ParseError> {
        let start = self.expect_word("return")?.span;
        let mut values = vec![self.parse_expr()?];
        while self.consume(&TokenKind::Comma).is_some() {
            values.push(self.parse_expr()?);
        }
        let end = self.expect_line_end()?;
        Ok(ReturnStmt {
            values,
            span: start.join(end),
        })
    }

    fn parse_if(&mut self) -> Result<IfStmt, ParseError> {
        let start = self.expect_word("if")?.span;
        let condition = self.parse_expr()?;
        let then_body = self.parse_statement_block()?;
        let mut else_body = Vec::new();
        let mut end = then_body.last().map_or(condition.span(), Stmt::span);
        if self.check_word("else") {
            self.next();
            else_body = self.parse_statement_block()?;
            end = else_body.last().map_or(self.previous_span(), Stmt::span);
        }
        Ok(IfStmt {
            condition,
            then_body,
            else_body,
            span: start.join(end),
        })
    }

    fn parse_match(&mut self) -> Result<MatchStmt, ParseError> {
        let start = self.expect_word("match")?.span;
        let value = self.parse_expr()?;
        self.open_block()?;
        let mut cases = Vec::new();
        while !self.check(&TokenKind::Dedent) {
            cases.push(self.parse_match_case()?);
        }
        let end = self.expect(TokenKind::Dedent)?.span;
        Ok(MatchStmt {
            value,
            cases,
            span: start.join(end),
        })
    }

    fn parse_match_case(&mut self) -> Result<MatchCase, ParseError> {
        let start = self.expect_word("case")?.span;
        let pattern = self.parse_pattern()?;
        let guard = if self.consume_word("if").is_some() {
            Some(self.parse_expr()?)
        } else {
            None
        };
        let body = self.parse_statement_block()?;
        let end = body.last().map_or(self.previous_span(), Stmt::span);
        Ok(MatchCase {
            pattern,
            guard,
            body,
            span: start.join(end),
        })
    }

    fn parse_for(&mut self) -> Result<ForStmt, ParseError> {
        let start = self.expect_word("for")?.span;
        let binding = self.take_identifier("a loop binding")?;
        self.expect_word("in")?;
        let iterable = self.parse_expr()?;
        let body = self.parse_statement_block()?;
        let end = body.last().map_or(self.previous_span(), Stmt::span);
        Ok(ForStmt {
            binding,
            iterable,
            body,
            span: start.join(end),
        })
    }

    fn parse_when(&mut self) -> Result<WhenStmt, ParseError> {
        let start = self.expect_word("when")?.span;
        let trigger = if self.consume_word("every").is_some() {
            Trigger::Every(self.parse_expr()?)
        } else if self.consume_word("after").is_some() {
            Trigger::After(self.parse_expr()?)
        } else {
            Trigger::Event(self.parse_expr()?)
        };
        let body = self.parse_statement_block()?;
        let end = body.last().map_or(self.previous_span(), Stmt::span);
        Ok(WhenStmt {
            trigger,
            body,
            span: start.join(end),
        })
    }

    fn parse_emit(&mut self) -> Result<EmitStmt, ParseError> {
        let start = self.expect_word("emit")?.span;
        let event = self.parse_expr()?;
        let end = self.expect_line_end()?;
        Ok(EmitStmt {
            event,
            span: start.join(end),
        })
    }

    fn parse_statement_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.open_block()?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::Dedent) {
            body.push(self.parse_stmt()?);
        }
        self.expect(TokenKind::Dedent)?;
        Ok(body)
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let path = self.parse_path()?;
        if self.consume(&TokenKind::LeftBrace).is_none() {
            let is_constructor = path.segments.len() != 1
                || path.segments[0]
                    .value
                    .starts_with(|character: char| character.is_ascii_uppercase());
            if is_constructor {
                return Ok(Pattern::Constructor {
                    span: path.span,
                    path,
                    fields: Vec::new(),
                });
            }
            return Ok(Pattern::Name(path.segments[0].clone()));
        }
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            let field = self.take_identifier("a pattern field")?;
            let binding = if self.consume(&TokenKind::Colon).is_some() {
                self.take_identifier("a pattern binding")?
            } else {
                field.clone()
            };
            let span = field.span.join(binding.span);
            fields.push(PatternField {
                field,
                binding,
                span,
            });
            if self.consume(&TokenKind::Comma).is_none() {
                break;
            }
        }
        let end = self.expect(TokenKind::RightBrace)?.span;
        let span = path.span.join(end);
        Ok(Pattern::Constructor { path, fields, span })
    }

    /// A typed field. `optional_allowed` is set where a field is something an
    /// author may leave unstated, which is an artifact kind's schema and
    /// nowhere else: a record's fields are all present in any value of it, and
    /// a workflow's inputs are all supplied by every call.
    fn parse_field_line(&mut self, optional_allowed: bool) -> Result<FieldDecl, ParseError> {
        let name = self.take_identifier("a field name")?;
        let optional = match self.consume(&TokenKind::Question) {
            Some(token) if !optional_allowed => {
                return Err(syntax_span(
                    token.span,
                    "only an artifact kind's schema states optional fields",
                ));
            }
            Some(_) => true,
            None => false,
        };
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let end = self.expect_line_end()?;
        Ok(FieldDecl {
            span: name.span.join(end),
            name,
            ty,
            optional,
        })
    }

    fn parse_type_parameters(&mut self) -> Result<Vec<TypeParameter>, ParseError> {
        if self.consume(&TokenKind::Less).is_none() {
            return Ok(Vec::new());
        }
        let mut parameters = Vec::new();
        while !self.check(&TokenKind::Greater) {
            let name = self.take_identifier("a type parameter")?;
            let bound = if self.consume(&TokenKind::Colon).is_some() {
                Some(self.parse_type()?)
            } else {
                None
            };
            let span = name
                .span
                .join(bound.as_ref().map_or(name.span, TypeExpr::span));
            parameters.push(TypeParameter { name, bound, span });
            if self.consume(&TokenKind::Comma).is_none() {
                break;
            }
        }
        self.expect(TokenKind::Greater)?;
        Ok(parameters)
    }

    fn parse_type(&mut self) -> Result<TypeExpr, ParseError> {
        let first = self.parse_type_primary()?;
        if self.consume(&TokenKind::Pipe).is_none() {
            return Ok(first);
        }
        let start = first.span();
        let mut alternatives = vec![first];
        loop {
            alternatives.push(self.parse_type_primary()?);
            if self.consume(&TokenKind::Pipe).is_none() {
                break;
            }
        }
        let end = alternatives.last().expect("non-empty").span();
        Ok(TypeExpr::Union {
            alternatives,
            span: start.join(end),
        })
    }

    /// A unit: a name, optionally over a denominator. One reader serves both
    /// `100 ng/uL` and `Quantity<ng/uL>`, so the two can never drift apart.
    fn parse_unit(&mut self) -> Result<String, ParseError> {
        let mut unit = self.take_identifier("a unit")?.value;
        if self.consume(&TokenKind::Slash).is_some() {
            unit.push('/');
            unit.push_str(&self.take_identifier("a unit denominator")?.value);
        }
        Ok(unit)
    }

    fn parse_type_primary(&mut self) -> Result<TypeExpr, ParseError> {
        // `any Signal` names some type playing a role. That is only meaningful
        // as an argument to another type: a value cannot be a signal, only
        // carry one.
        if self.check_word("any") {
            return Err(syntax_span(
                self.current_span(),
                "'any' is not a type on its own; write it as a type argument, such as Material<any Signal>",
            ));
        }
        let path = self.parse_path()?;
        // A quantity is measured in a unit, not parameterized by a type, so
        // `Quantity<ng/uL>` reads its argument exactly as `100 ng/uL` does.
        if path.segments.len() == 1
            && path.segments[0].value == "Quantity"
            && self.check(&TokenKind::Less)
        {
            self.next();
            let unit = self.parse_unit()?;
            let end = self.expect(TokenKind::Greater)?.span;
            return Ok(TypeExpr::Quantity {
                unit,
                span: path.span.join(end),
            });
        }
        let mut arguments = Vec::new();
        let mut end = path.span;
        if self.consume(&TokenKind::Less).is_some() {
            while !self.check(&TokenKind::Greater) {
                arguments.push(self.parse_type_argument()?);
                if self.consume(&TokenKind::Comma).is_none() {
                    break;
                }
            }
            end = self.expect(TokenKind::Greater)?.span;
        }
        Ok(TypeExpr::Path {
            span: path.span.join(end),
            path,
            arguments,
        })
    }

    /// A type argument, which may introduce the parameter it stands for.
    ///
    /// Inside `<...>` a colon cannot mean anything else, so `S: Signal` is
    /// unambiguously a binding rather than a type.
    fn parse_type_argument(&mut self) -> Result<TypeArgument, ParseError> {
        if self.check_word("any") {
            let start = self.next().expect("checked").span;
            let role = self.parse_path()?;
            return Ok(TypeArgument::Any {
                span: start.join(role.span),
                role,
            });
        }
        if let Some(TokenKind::Identifier(name)) = self.peek_kind(0)
            && self.peek_kind(1) == Some(&TokenKind::Colon)
        {
            let name = Spanned::new(name.clone(), self.current_span());
            self.cursor += 2;
            let role = self.parse_path()?;
            let span = name.span.join(role.span);
            return Ok(TypeArgument::Binding { name, role, span });
        }
        Ok(TypeArgument::Type(self.parse_type()?))
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_binary(0)
    }

    fn parse_binary(&mut self, minimum_precedence: u8) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        while let Some((op, precedence)) = self.binary_operator() {
            if precedence < minimum_precedence {
                break;
            }
            self.next();
            let right = self.parse_binary(precedence + 1)?;
            let span = left.span().join(right.span());
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.check(&TokenKind::Minus) || self.check_word("not") {
            let token = self.next().expect("checked");
            let op = if token.kind == TokenKind::Minus {
                UnaryOp::Negate
            } else {
                UnaryOp::Not
            };
            let operand = self.parse_unary()?;
            let span = token.span.join(operand.span());
            return Ok(Expr::Unary {
                op,
                operand: Box::new(operand),
                span,
            });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expression = self.parse_primary()?;
        loop {
            if self.consume(&TokenKind::LeftParen).is_some() {
                let mut arguments = Vec::new();
                while !self.check(&TokenKind::RightParen) {
                    let argument_start = self.current_span();
                    let name = if self.peek_identifier().is_some()
                        && self.peek_kind(1) == Some(&TokenKind::Colon)
                    {
                        let name = self.take_identifier("an argument name")?;
                        self.expect(TokenKind::Colon)?;
                        Some(name)
                    } else {
                        None
                    };
                    let value = self.parse_expr()?;
                    arguments.push(Argument {
                        span: argument_start.join(value.span()),
                        name,
                        value,
                    });
                    if self.consume(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                let end = self.expect(TokenKind::RightParen)?.span;
                let span = expression.span().join(end);
                expression = Expr::Call {
                    callee: Box::new(expression),
                    arguments,
                    span,
                };
            } else if self.check(&TokenKind::LeftBrace) {
                let Expr::Path(constructor) = expression else {
                    return Err(syntax_span(
                        self.current_span(),
                        "only a named constructor may be followed by '{'",
                    ));
                };
                self.next();
                let mut fields = Vec::new();
                while !self.check(&TokenKind::RightBrace) {
                    let name = self.take_identifier("a constructor field")?;
                    self.expect(TokenKind::Colon)?;
                    let value = self.parse_expr()?;
                    fields.push(FieldValue {
                        span: name.span.join(value.span()),
                        name,
                        value,
                    });
                    if self.consume(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                let end = self.expect(TokenKind::RightBrace)?.span;
                let span = constructor.span.join(end);
                expression = Expr::Record {
                    constructor,
                    fields,
                    span,
                };
            } else if self.consume(&TokenKind::Dot).is_some() {
                let field = self.take_identifier("a field name")?;
                let span = expression.span().join(field.span);
                expression = Expr::Field {
                    subject: Box::new(expression),
                    field,
                    span,
                };
            } else if is_numeric(&expression) && self.peek_identifier().is_some() {
                let unit_start = self.current_span();
                let unit = self.parse_unit()?;
                let end = self.previous_span();
                let span = expression.span().join(end);
                debug_assert!(unit_start.start < span.end);
                expression = Expr::Quantity {
                    magnitude: Box::new(expression),
                    unit,
                    span,
                };
            } else {
                break;
            }
        }
        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let token = self
            .next()
            .ok_or_else(|| syntax_span(Span::at(self.source.len()), "expected an expression"))?;
        match token.kind {
            TokenKind::Identifier(value) => {
                let first = Spanned::new(value, token.span);
                let mut segments = vec![first];
                while self.check(&TokenKind::Dot)
                    && matches!(self.peek_kind(1), Some(TokenKind::Identifier(_)))
                {
                    self.next();
                    segments.push(self.take_identifier("a path segment")?);
                }
                let end = segments.last().expect("non-empty").span;
                Ok(Expr::Path(Path {
                    segments,
                    span: token.span.join(end),
                }))
            }
            TokenKind::Integer(value) => Ok(Expr::Integer {
                value,
                span: token.span,
            }),
            TokenKind::Decimal(text) => Ok(Expr::Decimal {
                text,
                span: token.span,
            }),
            TokenKind::String(value) => Ok(Expr::String {
                value,
                span: token.span,
            }),
            TokenKind::LeftParen => {
                let expression = self.parse_expr()?;
                self.expect(TokenKind::RightParen)?;
                Ok(expression)
            }
            TokenKind::LeftBracket => {
                let mut elements = Vec::new();
                while !self.check(&TokenKind::RightBracket) {
                    elements.push(self.parse_expr()?);
                    if self.consume(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
                let end = self.expect(TokenKind::RightBracket)?.span;
                Ok(Expr::List {
                    elements,
                    span: token.span.join(end),
                })
            }
            found => Err(syntax_span(
                token.span,
                format!("expected an expression, found {found}"),
            )),
        }
    }

    fn parse_path(&mut self) -> Result<Path, ParseError> {
        let first = self.take_identifier("a path")?;
        let start = first.span;
        let mut segments = vec![first];
        while self.consume(&TokenKind::Dot).is_some() {
            segments.push(self.take_identifier("a path segment")?);
        }
        let end = segments.last().expect("non-empty").span;
        Ok(Path {
            segments,
            span: start.join(end),
        })
    }

    fn binary_operator(&self) -> Option<(BinaryOp, u8)> {
        match self.peek_kind(0)? {
            TokenKind::Identifier(word) if word == "or" => Some((BinaryOp::Or, 1)),
            TokenKind::Identifier(word) if word == "and" => Some((BinaryOp::And, 2)),
            TokenKind::EqualEqual => Some((BinaryOp::Equal, 3)),
            TokenKind::NotEqual => Some((BinaryOp::NotEqual, 3)),
            TokenKind::Less => Some((BinaryOp::Less, 3)),
            TokenKind::LessEqual => Some((BinaryOp::LessEqual, 3)),
            TokenKind::Greater => Some((BinaryOp::Greater, 3)),
            TokenKind::GreaterEqual => Some((BinaryOp::GreaterEqual, 3)),
            TokenKind::DotDot => Some((BinaryOp::Range, 4)),
            TokenKind::Plus => Some((BinaryOp::Add, 5)),
            TokenKind::Minus => Some((BinaryOp::Subtract, 5)),
            TokenKind::Star => Some((BinaryOp::Multiply, 6)),
            TokenKind::Slash => Some((BinaryOp::Divide, 6)),
            _ => None,
        }
    }

    fn open_block(&mut self) -> Result<(), ParseError> {
        self.expect(TokenKind::Colon)?;
        self.expect(TokenKind::Newline)?;
        self.expect(TokenKind::Indent)?;
        Ok(())
    }

    fn expect_line_end(&mut self) -> Result<Span, ParseError> {
        self.expect(TokenKind::Newline).map(|token| token.span)
    }

    fn skip_newlines(&mut self) {
        while self.consume(&TokenKind::Newline).is_some() {}
    }

    /// Take the module's own documentation, which opens its file. A module
    /// describes the file it is, so `/*! */` anywhere else has no subject.
    fn take_module_doc(&mut self) -> Result<Option<String>, ParseError> {
        let Some(TokenKind::ModuleDoc(text)) = self.peek_kind(0) else {
            return Ok(None);
        };
        let text = text.clone();
        self.cursor += 1;
        self.skip_newlines();
        Ok(Some(text))
    }

    /// Take the documentation comment standing above the declaration about to
    /// be parsed. Documentation always describes the declaration that follows
    /// it, so one that describes nothing is an error rather than a comment.
    fn take_doc(&mut self) -> Result<Option<String>, ParseError> {
        if matches!(self.peek_kind(0), Some(TokenKind::ModuleDoc(_))) {
            return Err(syntax_span(
                self.current_span(),
                "a module's documentation opens its file; use '/** */' to document a declaration",
            ));
        }
        let Some(TokenKind::DocComment(text)) = self.peek_kind(0) else {
            return Ok(None);
        };
        let text = text.clone();
        let span = self.current_span();
        self.cursor += 1;
        self.skip_newlines();
        if self.at_end() {
            return Err(syntax_span(
                span,
                "this documentation comment describes no declaration",
            ));
        }
        if matches!(self.peek_kind(0), Some(TokenKind::DocComment(_))) {
            return Err(syntax_span(
                self.current_span(),
                "a declaration takes one documentation comment; write the whole description in one '/** */'",
            ));
        }
        Ok(Some(text))
    }

    fn line_has(&self, kind: TokenKind) -> bool {
        self.tokens[self.cursor..]
            .iter()
            .take_while(|token| token.kind != TokenKind::Newline)
            .any(|token| token.kind == kind)
    }

    fn check_word(&self, expected: &str) -> bool {
        matches!(self.peek_kind(0), Some(TokenKind::Identifier(word)) if word == expected)
    }

    fn consume_word(&mut self, expected: &str) -> Option<Token> {
        self.check_word(expected)
            .then(|| self.next().expect("checked"))
    }

    fn expect_word(&mut self, expected: &str) -> Result<Token, ParseError> {
        let token = self.next().ok_or_else(|| {
            syntax_span(
                Span::at(self.source.len()),
                format!("expected '{expected}', found end of input"),
            )
        })?;
        match &token.kind {
            TokenKind::Identifier(word) if word == expected => Ok(token),
            found => Err(syntax_span(
                token.span,
                format!("expected '{expected}', found {found}"),
            )),
        }
    }

    fn take_identifier(&mut self, expected: &str) -> Result<Identifier, ParseError> {
        let token = self.next().ok_or_else(|| {
            syntax_span(
                Span::at(self.source.len()),
                format!("expected {expected}, found end of input"),
            )
        })?;
        match token.kind {
            TokenKind::Identifier(value) => Ok(Spanned::new(value, token.span)),
            found => Err(syntax_span(
                token.span,
                format!("expected {expected}, found {found}"),
            )),
        }
    }

    fn expect(&mut self, expected: TokenKind) -> Result<Token, ParseError> {
        let token = self.next().ok_or_else(|| {
            syntax_span(
                Span::at(self.source.len()),
                format!("expected {expected}, found end of input"),
            )
        })?;
        if token.kind == expected {
            Ok(token)
        } else {
            Err(syntax_span(
                token.span,
                format!("expected {expected}, found {}", token.kind),
            ))
        }
    }

    fn consume(&mut self, expected: &TokenKind) -> Option<Token> {
        self.check(expected).then(|| self.next().expect("checked"))
    }

    fn check(&self, expected: &TokenKind) -> bool {
        self.peek_kind(0) == Some(expected)
    }

    fn peek_identifier(&self) -> Option<&str> {
        match self.peek_kind(0) {
            Some(TokenKind::Identifier(word)) => Some(word),
            _ => None,
        }
    }

    fn peek_kind(&self, distance: usize) -> Option<&TokenKind> {
        self.tokens
            .get(self.cursor + distance)
            .map(|token| &token.kind)
    }

    fn current_span(&self) -> Span {
        self.tokens
            .get(self.cursor)
            .map_or(Span::at(self.source.len()), |token| token.span)
    }

    fn previous_span(&self) -> Span {
        self.cursor
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
            .map_or(Span::at(0), |token| token.span)
    }

    fn at_end(&self) -> bool {
        self.cursor == self.tokens.len()
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.cursor).cloned()?;
        self.cursor += 1;
        Some(token)
    }
}

fn is_numeric(expression: &Expr) -> bool {
    matches!(
        expression,
        Expr::Integer { .. }
            | Expr::Decimal { .. }
            | Expr::Unary {
                op: UnaryOp::Negate,
                ..
            }
    )
}

#[cfg(test)]
mod tests {
    use crate::parser::*;

    #[test]
    fn parses_reactive_workflows_without_pretending_to_lower_them() {
        let source = r#"
use std.lab.plasmid

record ColonyGrowth:
  plate: Material<Plate>
  case Ready:
    colonies: ColonyMap
  case TimedOut

workflow await_colonies(plate: Material<Plate>) -> ColonyGrowth:
  state observations: List<PlateObservation> = []

  when every 30 min:
    image <- capture image of plate
    colonies = detect_colonies(image)
    if colonies.isolated.count >= 8:
      return Ready{
        plate: plate,
        colonies: colonies,
      }

  when after 18 h:
    return TimedOut{plate: plate}
"#;

        let module = parse_module(source).unwrap();
        assert_eq!(module.items.len(), 3);
        let Item::Workflow(workflow) = &module.items[2] else {
            panic!("expected workflow")
        };
        assert_eq!(workflow.inputs.len(), 1);
        assert_eq!(workflow.inputs[0].name.value, "plate");
        assert!(matches!(
            &workflow.outputs,
            WorkflowOutputs::Single {
                ty: TypeExpr::Path { path, .. },
            } if path.segments[0].value == "ColonyGrowth"
        ));
        assert_eq!(workflow.body.len(), 3);
        assert!(matches!(workflow.body[0], Stmt::State(_)));
        assert!(matches!(workflow.body[1], Stmt::When(_)));
        assert!(matches!(workflow.body[2], Stmt::When(_)));
    }

    #[test]
    fn parses_named_workflow_results_and_direct_returns() {
        let module = parse_module(
            r#"workflow preserve(
  product: Material<Plasmid>,
  plate: Material<Plate>,
) -> (
  product: Material<Plasmid>,
  plate: Material<Plate>,
):
  return product, plate
"#,
        )
        .unwrap();
        let Item::Workflow(workflow) = &module.items[0] else {
            panic!("expected workflow")
        };
        let WorkflowOutputs::Named { fields } = &workflow.outputs else {
            panic!("expected named workflow results")
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name.value, "product");
        assert_eq!(fields[1].name.value, "plate");
        let Stmt::Return(statement) = &workflow.body[0] else {
            panic!("expected return")
        };
        assert_eq!(statement.values.len(), 2);
    }

    #[test]
    fn rejects_empty_named_workflow_results() {
        let error = parse_module("workflow empty() -> ():\n  return None\n").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("named workflow result list cannot be empty"),
            "{error}"
        );
    }

    #[test]
    fn rejects_body_level_workflow_signatures() {
        let error = parse_module(
            "workflow legacy:\n  input sample: Material<Plasmid>\n  output None\n  return None\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("expected '('"), "{error}");
    }

    #[test]
    fn representative_language_specimens_remain_parseable() {
        let design = parse_module(include_str!(
            "../../../docs/language/specimens/plasmid-design.lab"
        ))
        .unwrap();
        assert!(
            design
                .items
                .iter()
                .any(|item| matches!(item, Item::Circuit(_)))
        );

        let panel = parse_module(include_str!(
            "../../../docs/language/specimens/sensor-panel.lab"
        ))
        .unwrap();
        assert!(
            panel
                .items
                .iter()
                .any(|item| matches!(item, Item::Data(data) if data.name.value == "Reading"))
        );

        let build = parse_module(include_str!(
            "../../../docs/language/specimens/plasmid-build.lab"
        ))
        .unwrap();
        assert!(
            build
                .items
                .iter()
                .any(|item| matches!(item, Item::Workflow(_)))
        );
    }

    /// A property associates a name with a value and a field gives a name a
    /// type. One token after the name decides which, without knowing the word
    /// that opened the block — which is what lets a declaration's shape be
    /// parsed before its meaning is resolved.
    #[test]
    fn a_property_and_a_field_are_distinguished_without_keyword_context() {
        let module = parse_module("plasmid p:\n  sequence = dna(\"ACGT\")\n").unwrap();
        let Item::Artifact(artifact) = &module.items[0] else {
            panic!("expected the artifact")
        };
        assert!(matches!(artifact.members[0], ArtifactMember::Property(_)));

        let module = parse_module("record Plate:\n  colonies: ColonyMap\n").unwrap();
        let Item::Data(data) = &module.items[0] else {
            panic!("expected the record")
        };
        assert_eq!(data.fields[0].name.value, "colonies");
    }

    #[test]
    fn rejects_type_annotation_syntax_for_a_property() {
        let error = parse_module(
            "use std.bio.designs\n\nplasmid legacy:\n  sequence: dna(\"ACGT\")\n  require topology == circular\n",
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("expected a newline"),
            "a property takes a value, so ':' opens a section instead: {error}"
        );
    }

    #[test]
    fn derives_the_instance_word_from_the_type_name() {
        for (type_name, word) in [
            ("Plasmid", "plasmid"),
            ("RestrictionEnzyme", "restriction_enzyme"),
            // An acronym is one word, not one word per letter.
            ("DNA", "dna"),
            ("CDS", "cds"),
            ("DNASequence", "dna_sequence"),
        ] {
            let module =
                parse_module(&format!("artifact {type_name}:\n  label: String\n")).unwrap();
            let Item::ArtifactKind(kind) = &module.items[0] else {
                panic!("the item is an artifact kind");
            };
            assert_eq!(kind.name.value, word, "instances of {type_name}");
        }
    }

    #[test]
    fn marks_an_optional_schema_field_on_its_name() {
        let module = parse_module("artifact Plasmid:\n  label: String\n  note?: String\n").unwrap();
        let Item::ArtifactKind(kind) = &module.items[0] else {
            panic!("the item is an artifact kind");
        };
        assert!(!kind.fields[0].optional);
        assert!(kind.fields[1].optional);
    }

    #[test]
    fn rejects_an_optional_record_field() {
        let error = parse_module("record Sample:\n  label?: String\n").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("only an artifact kind's schema states optional fields"),
            "every value of a record carries every field: {error}"
        );
    }

    #[test]
    fn rejects_an_optional_workflow_parameter() {
        let error =
            parse_module("workflow run(sample?: String) -> None:\n  return None\n").unwrap_err();
        assert!(
            error.to_string().contains("a parameter is never optional"),
            "every call supplies every parameter: {error}"
        );
    }

    /// A ':' ending the header opens a block, on an artifact instance as
    /// everywhere else, and an ascribed instance opens one after its type the
    /// way a workflow does after its result.
    #[test]
    fn an_artifact_block_is_opened_by_a_colon() {
        let error = parse_module("buy enzyme BsaI\n  digest_temperature = 37 C\n").unwrap_err();
        assert!(
            error.to_string().contains("opened by ':'"),
            "an indented block without ':' is refused: {error}"
        );

        let module = parse_module(
            "buy promoter pTet: Promoter<Tetracycline>:\n  identity = \"BBa_R0040\"\n",
        )
        .unwrap();
        let Item::Artifact(artifact) = &module.items[0] else {
            panic!("the item is an artifact instance");
        };
        assert!(artifact.ascribed.is_some());
        assert_eq!(artifact.members.len(), 1);
    }

    /// `buy:` states one provenance over a block of instances. Each line
    /// inside is the instance form without a verb, and each becomes its own
    /// declaration — the block is grouping, not a node.
    #[test]
    fn a_provenance_block_states_one_origin_over_every_instance_inside() {
        let module = parse_module(
            "buy:\n  part J23101\n  part B0034\n  backbone pSB1C3\n  restriction_enzyme BsaI:\n    digest_temperature = 37 C\n\nbuild plasmid reporter:\n  backbone = pSB1C3\n",
        )
        .unwrap();
        assert_eq!(module.items.len(), 5);
        for (index, name) in ["J23101", "B0034", "pSB1C3", "BsaI"].iter().enumerate() {
            let Item::Artifact(artifact) = &module.items[index] else {
                panic!("expected an artifact instance for {name}");
            };
            assert_eq!(artifact.provenance, Provenance::Buy);
            assert_eq!(artifact.name.value, *name);
        }
        let Item::Artifact(enzyme) = &module.items[3] else {
            panic!("expected the enzyme");
        };
        assert_eq!(enzyme.members.len(), 1, "an instance keeps its own block");
        let Item::Artifact(reporter) = &module.items[4] else {
            panic!("expected the plasmid");
        };
        assert_eq!(reporter.provenance, Provenance::Build);
    }

    #[test]
    fn a_build_block_and_an_ascribed_instance_work_inside_a_provenance_block() {
        let module = parse_module(
            "buy:\n  promoter pTet: Promoter<Tetracycline>\n\nbuild:\n  plasmid p:\n    sequence = dna(\"ACGT\")\n",
        )
        .unwrap();
        let Item::Artifact(promoter) = &module.items[0] else {
            panic!("expected the promoter");
        };
        assert_eq!(promoter.provenance, Provenance::Buy);
        assert!(promoter.ascribed.is_some());
        let Item::Artifact(plasmid) = &module.items[1] else {
            panic!("expected the plasmid");
        };
        assert_eq!(plasmid.provenance, Provenance::Build);
        assert_eq!(plasmid.members.len(), 1);
    }

    #[test]
    fn a_verb_inside_a_provenance_block_is_refused() {
        let error = parse_module("buy:\n  buy part J23101\n").unwrap_err();
        assert!(
            error.to_string().contains("without a verb"),
            "the block already states the provenance: {error}"
        );
    }

    #[test]
    fn documentation_inside_a_provenance_block_attaches_per_instance() {
        let module = parse_module(
            "buy:\n  /** The strong constitutive promoter. */\n  part J23101\n  part B0034\n",
        )
        .unwrap();
        let Item::Artifact(first) = &module.items[0] else {
            panic!("expected the first part");
        };
        assert_eq!(
            first.doc.as_deref(),
            Some("The strong constitutive promoter.")
        );
        let Item::Artifact(second) = &module.items[1] else {
            panic!("expected the second part");
        };
        assert!(second.doc.is_none());

        let error = parse_module("/** Everything bought. */\nbuy:\n  part J23101\n").unwrap_err();
        assert!(
            error.to_string().contains("each thing inside the block"),
            "{error}"
        );
    }

    #[test]
    fn documentation_attaches_to_the_declaration_below_it() {
        let module = parse_module(
            "use std.bio.build\n\n/**\n * Assemble the reporter plasmid.\n *\n * Takes no material input.\n */\nworkflow assemble() -> Material<Plasmid>:\n  product <- realize reporter\n  return product\n",
        )
        .unwrap();

        let Item::Workflow(workflow) = &module.items[1] else {
            panic!("the second item is the documented workflow");
        };
        assert_eq!(
            workflow.doc.as_deref(),
            Some("Assemble the reporter plasmid.\n\nTakes no material input.")
        );
        assert!(
            matches!(&module.items[0], Item::Use(_)),
            "documentation does not become an item of its own"
        );
    }

    #[test]
    fn documentation_reaches_every_kind_of_declaration() {
        let module = parse_module(
            "/** A reporter. */\nplasmid reporter:\n  sequence = dna(\"ACGT\")\n\n/** A count. */\ntotal = 3\n",
        )
        .unwrap();
        let Item::Artifact(artifact) = &module.items[0] else {
            panic!("the first item is a plasmid");
        };
        assert_eq!(artifact.doc.as_deref(), Some("A reporter."));
        let Item::Binding(binding) = &module.items[1] else {
            panic!("the second item is a binding");
        };
        assert_eq!(binding.doc.as_deref(), Some("A count."));
    }

    #[test]
    fn module_documentation_opens_the_file_and_belongs_to_the_module() {
        let module = parse_module(
            "/*!\n * Four engineered organisms.\n */\n\nuse golden_gate.designs.plasmids\n\n/** The first. */\nstrain first:\n  chassis = DH5alpha\n",
        )
        .unwrap();

        assert_eq!(module.doc.as_deref(), Some("Four engineered organisms."));
        let Item::Artifact(artifact) = &module.items[1] else {
            panic!("the strain follows the import");
        };
        assert_eq!(
            artifact.doc.as_deref(),
            Some("The first."),
            "module documentation does not claim the first declaration's own"
        );
    }

    #[test]
    fn rejects_module_documentation_that_does_not_open_the_file() {
        let error = parse_module(
            "use std.bio.build\nuse std.bio.designs\n\n/*! Too late. */\nstrain first:\n  chassis = DH5alpha\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("opens its file"), "{error}");
    }

    #[test]
    fn rejects_documentation_that_describes_no_declaration() {
        let dangling = parse_module("total = 3\n\n/** Describes nothing. */\n").unwrap_err();
        assert!(
            dangling.to_string().contains("describes no declaration"),
            "{dangling}"
        );

        let import = parse_module("/** Describes an import. */\nuse std.bio.build\n").unwrap_err();
        assert!(import.to_string().contains("not an import"), "{import}");

        let doubled = parse_module("/** First. */\n/** Second. */\ntotal = 3\n").unwrap_err();
        assert!(
            doubled.to_string().contains("one documentation"),
            "{doubled}"
        );
    }
}
