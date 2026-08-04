use super::ast::*;
use super::error::{ParseError, syntax_span};
use super::lexer::lex;
use super::source::{Identifier, Span, Spanned};
use super::token::{Token, TokenKind};

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
        while !self.at_end() {
            let item = if self.check_word("use") {
                Item::Use(self.parse_use()?)
            } else if self.check_word("circuit") {
                Item::Circuit(self.parse_circuit()?)
            } else if self.check_word("plasmid") {
                Item::Plasmid(self.parse_plasmid()?)
            } else if let Some(kind) = self.data_kind() {
                Item::Data(self.parse_data(kind)?)
            } else if self.check_word("workflow") {
                Item::Workflow(self.parse_workflow()?)
            } else {
                Item::Binding(self.parse_binding()?)
            };
            items.push(item);
            self.skip_newlines();
        }
        Ok(Module {
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

    fn parse_circuit(&mut self) -> Result<CircuitDecl, ParseError> {
        let start = self.expect_word("circuit")?.span;
        let name = self.take_identifier("a circuit name")?;
        let parameters = self.parse_type_parameters()?;
        self.open_block()?;

        let mut inputs = Vec::new();
        let mut output = None;
        let mut sections = Vec::new();
        while !self.check(&TokenKind::Dedent) {
            if self.check_word("input") {
                self.next();
                inputs.push(self.parse_field_line()?);
            } else if self.check_word("output") {
                let keyword = self.next().expect("checked");
                if output.is_some() {
                    return Err(syntax_span(keyword.span, "duplicate circuit output"));
                }
                output = Some(self.parse_type()?);
                self.expect_line_end()?;
            } else {
                sections.push(self.parse_section()?);
            }
        }
        let end = self.expect(TokenKind::Dedent)?.span;
        Ok(CircuitDecl {
            name,
            parameters,
            inputs,
            output,
            sections,
            span: start.join(end),
        })
    }

    fn parse_plasmid(&mut self) -> Result<PlasmidDecl, ParseError> {
        let start = self.expect_word("plasmid")?.span;
        let name = self.take_identifier("a plasmid name")?;
        self.open_block()?;
        let mut members = Vec::new();
        while !self.check(&TokenKind::Dedent) {
            if self.check_word("require") || self.check_word("accept") {
                let acceptance = self.check_word("accept");
                let keyword = self.next().expect("checked");
                let predicate = self.parse_expr()?;
                let end = self.expect_line_end()?;
                let claim = ClaimStmt {
                    predicate,
                    span: keyword.span.join(end),
                };
                members.push(if acceptance {
                    PlasmidMember::Acceptance(claim)
                } else {
                    PlasmidMember::Requirement(claim)
                });
            } else if self.line_has(TokenKind::Equal) {
                members.push(PlasmidMember::Binding(self.parse_binding()?));
            } else {
                members.push(PlasmidMember::Section(self.parse_section()?));
            }
        }
        let end = self.expect(TokenKind::Dedent)?.span;
        Ok(PlasmidDecl {
            name,
            members,
            span: start.join(end),
        })
    }

    fn parse_data(&mut self, kind: DataKind) -> Result<DataDecl, ParseError> {
        let keyword = self.next().expect("data kind was checked");
        let name = self.take_identifier("a declaration name")?;
        let parameters = self.parse_type_parameters()?;
        self.open_block()?;
        let mut fields = Vec::new();
        let mut cases = Vec::new();
        while !self.check(&TokenKind::Dedent) {
            if self.check_word("case") {
                cases.push(self.parse_case_decl()?);
            } else {
                fields.push(self.parse_field_line()?);
            }
        }
        let end = self.expect(TokenKind::Dedent)?.span;
        Ok(DataDecl {
            kind,
            name,
            parameters,
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
                fields.push(self.parse_field_line()?);
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
        self.open_block()?;
        let mut inputs = Vec::new();
        let mut output = None;
        let mut body = Vec::new();
        while !self.check(&TokenKind::Dedent) {
            if self.check_word("input") {
                self.next();
                inputs.push(self.parse_field_line()?);
            } else if self.check_word("output") {
                let keyword = self.next().expect("checked");
                if output.is_some() {
                    return Err(syntax_span(keyword.span, "duplicate workflow output"));
                }
                output = Some(self.parse_type()?);
                self.expect_line_end()?;
            } else {
                body.push(self.parse_stmt()?);
            }
        }
        let end = self.expect(TokenKind::Dedent)?.span;
        Ok(WorkflowDecl {
            name,
            inputs,
            output,
            body,
            span: start.join(end),
        })
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
            names: vec![name],
            annotation,
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
        Ok(EffectStmt {
            names,
            action: self.source[action_start..action_end].trim().to_owned(),
            span: start.join(end),
        })
    }

    fn parse_return(&mut self) -> Result<ReturnStmt, ParseError> {
        let start = self.expect_word("return")?.span;
        let value = self.parse_expr()?;
        let end = self.expect_line_end()?;
        Ok(ReturnStmt {
            value,
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

    fn parse_field_line(&mut self) -> Result<FieldDecl, ParseError> {
        let name = self.take_identifier("a field name")?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let end = self.expect_line_end()?;
        Ok(FieldDecl {
            span: name.span.join(end),
            name,
            ty,
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

    fn parse_type_primary(&mut self) -> Result<TypeExpr, ParseError> {
        let path = self.parse_path()?;
        let mut arguments = Vec::new();
        let mut end = path.span;
        if self.consume(&TokenKind::Less).is_some() {
            while !self.check(&TokenKind::Greater) {
                arguments.push(self.parse_type()?);
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
                let mut unit = self.take_identifier("a unit")?.value;
                let mut end = self.previous_span();
                if self.consume(&TokenKind::Slash).is_some() {
                    unit.push('/');
                    let denominator = self.take_identifier("a unit denominator")?;
                    unit.push_str(&denominator.value);
                    end = denominator.span;
                }
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

    fn data_kind(&self) -> Option<DataKind> {
        let word = self.peek_identifier()?;
        match word {
            "record" => Some(DataKind::Record),
            "material" => Some(DataKind::Material),
            "observation" => Some(DataKind::Observation),
            "evidence" => Some(DataKind::Evidence),
            "event" => Some(DataKind::Event),
            "outcome" => Some(DataKind::Outcome),
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
    use super::*;
    use crate::{AcceptanceCriterion, Artifact, SpecError, Topology, parse};

    #[test]
    fn parses_and_lowers_a_declarative_plasmid() {
        let source = r#"
# implementation details are intentionally absent
plasmid p_sensor:
  sequence = dna("acgtacgt")
  require topology == circular
  accept sequence == design.sequence
  accept concentration >= 100 ng/uL
  accept volume >= 20 uL
"#;

        let spec = parse(source).unwrap();
        assert_eq!(spec.name(), "p_sensor");
        assert_eq!(spec.copies().get(), 1);
        assert_eq!(spec.acceptance().len(), 3);
        assert!(
            spec.acceptance()
                .contains(&AcceptanceCriterion::ExactSequence)
        );
        let Artifact::Plasmid(plasmid) = spec.artifact();
        assert_eq!(plasmid.sequence().as_str(), "ACGTACGT");
        assert_eq!(plasmid.topology(), Topology::Circular);
    }

    #[test]
    fn parses_reactive_workflows_without_pretending_to_lower_them() {
        let source = r#"
use std.lab.plasmid_actions

outcome ColonyGrowth:
  plate: Material<Plate>
  case Ready:
    colonies: ColonyMap
  case TimedOut

workflow await_colonies:
  input plate: Material<Plate>
  output ColonyGrowth
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
        assert_eq!(workflow.body.len(), 3);
        assert!(matches!(workflow.body[0], Stmt::State(_)));
        assert!(matches!(workflow.body[1], Stmt::When(_)));
        assert!(matches!(workflow.body[2], Stmt::When(_)));

        let error = parse(source).unwrap_err();
        assert!(matches!(error, ParseError::Unsupported { .. }));
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

    #[test]
    fn rejects_a_plan_without_verification_evidence() {
        let error = parse(
            r#"plasmid p_unverified:
  sequence = dna("ACGT")
  accept volume >= 20 uL
"#,
        )
        .unwrap_err();

        assert_eq!(
            error,
            ParseError::Specification(SpecError::MissingSequenceAcceptance)
        );
    }
}
