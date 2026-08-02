use super::{
    AcceptanceCriterion, ArtifactSpec, Concentration, DnaSequence, PlasmidSpec, Topology, Volume,
};

use super::error::{ParseError, syntax};
use super::lexer::lex;
use super::token::{Token, TokenKind};

/// Parse one biological artifact specification from Lab Lang source.
pub fn parse(source: &str) -> Result<ArtifactSpec, ParseError> {
    let tokens = lex(source)?;
    Parser::new(tokens, source.len()).parse_artifact()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    eof_offset: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>, eof_offset: usize) -> Self {
        Self {
            tokens,
            cursor: 0,
            eof_offset,
        }
    }

    fn parse_artifact(mut self) -> Result<ArtifactSpec, ParseError> {
        self.expect_word("plasmid")?;
        let name = self.take_word("an artifact name")?;
        self.expect(TokenKind::LeftBrace)?;

        let mut sequence = None;
        let mut topology = None;
        let mut copies = None;
        let mut acceptance = None;

        while !self.check(&TokenKind::RightBrace) {
            let field_offset = self.current_offset();
            match self.take_word("a plasmid field")?.as_str() {
                "sequence" => {
                    if sequence.is_some() {
                        return Err(syntax(field_offset, "duplicate sequence field"));
                    }
                    sequence = Some(self.take_string("a quoted DNA sequence")?);
                    self.expect(TokenKind::Semicolon)?;
                }
                "topology" => {
                    if topology.is_some() {
                        return Err(syntax(field_offset, "duplicate topology field"));
                    }
                    let value = self.take_word("a topology")?;
                    topology = Some(match value.as_str() {
                        "circular" => Topology::Circular,
                        "linear" => Topology::Linear,
                        _ => {
                            return Err(syntax(
                                field_offset,
                                format!("unknown topology '{value}'"),
                            ));
                        }
                    });
                    self.expect(TokenKind::Semicolon)?;
                }
                "copies" => {
                    if copies.is_some() {
                        return Err(syntax(field_offset, "duplicate copies field"));
                    }
                    let value = self.take_number("a positive copy count")?;
                    copies = Some(u16::try_from(value).map_err(|_| {
                        syntax(field_offset, "copy count exceeds the supported u16 range")
                    })?);
                    self.expect(TokenKind::Semicolon)?;
                }
                "acceptance" => {
                    if acceptance.is_some() {
                        return Err(syntax(field_offset, "duplicate acceptance block"));
                    }
                    acceptance = Some(self.parse_acceptance()?);
                }
                unknown => {
                    return Err(syntax(
                        field_offset,
                        format!("unknown plasmid field '{unknown}'"),
                    ));
                }
            }
        }
        self.expect(TokenKind::RightBrace)?;
        if self.cursor != self.tokens.len() {
            return Err(syntax(
                self.current_offset(),
                "only one artifact specification is currently supported",
            ));
        }

        let sequence = sequence.ok_or_else(|| syntax(self.eof_offset, "missing sequence field"))?;
        let sequence = DnaSequence::new(sequence)?;
        let plasmid = PlasmidSpec::new(sequence, topology.unwrap_or(Topology::Circular))?;
        ArtifactSpec::plasmid(
            name,
            plasmid,
            copies.unwrap_or(1),
            acceptance.unwrap_or_default(),
        )
        .map_err(ParseError::from)
    }

    fn parse_acceptance(&mut self) -> Result<Vec<AcceptanceCriterion>, ParseError> {
        self.expect(TokenKind::LeftBrace)?;
        let mut criteria = Vec::new();
        while !self.check(&TokenKind::RightBrace) {
            let offset = self.current_offset();
            match self.take_word("an acceptance criterion")?.as_str() {
                "exact_sequence" => {
                    criteria.push(AcceptanceCriterion::ExactSequence);
                    self.expect(TokenKind::Semicolon)?;
                }
                "minimum_concentration" => {
                    let value = self.take_u32("a concentration value", offset)?;
                    self.expect_word("ng_per_ul")?;
                    self.expect(TokenKind::Semicolon)?;
                    criteria.push(AcceptanceCriterion::MinimumConcentration {
                        concentration: Concentration::nanograms_per_microliter(value),
                    });
                }
                "minimum_volume" => {
                    let value = self.take_u32("a volume value", offset)?;
                    self.expect_word("ul")?;
                    self.expect(TokenKind::Semicolon)?;
                    criteria.push(AcceptanceCriterion::MinimumVolume {
                        volume: Volume::microliters(value),
                    });
                }
                unknown => {
                    return Err(syntax(
                        offset,
                        format!("unknown acceptance criterion '{unknown}'"),
                    ));
                }
            }
        }
        self.expect(TokenKind::RightBrace)?;
        Ok(criteria)
    }

    fn take_u32(&mut self, expected: &str, offset: usize) -> Result<u32, ParseError> {
        let value = self.take_number(expected)?;
        u32::try_from(value).map_err(|_| syntax(offset, format!("{expected} exceeds u32")))
    }

    fn expect_word(&mut self, expected: &str) -> Result<(), ParseError> {
        let token = self.next().ok_or_else(|| {
            syntax(
                self.eof_offset,
                format!("expected '{expected}', found end of input"),
            )
        })?;
        match token.kind {
            TokenKind::Word(ref word) if word == expected => Ok(()),
            found => Err(syntax(
                token.offset,
                format!("expected '{expected}', found {found}"),
            )),
        }
    }

    fn expect(&mut self, expected: TokenKind) -> Result<(), ParseError> {
        let token = self.next().ok_or_else(|| {
            syntax(
                self.eof_offset,
                format!("expected {expected}, found end of input"),
            )
        })?;
        if token.kind == expected {
            Ok(())
        } else {
            Err(syntax(
                token.offset,
                format!("expected {expected}, found {}", token.kind),
            ))
        }
    }

    fn take_word(&mut self, expected: &str) -> Result<String, ParseError> {
        let token = self.next().ok_or_else(|| {
            syntax(
                self.eof_offset,
                format!("expected {expected}, found end of input"),
            )
        })?;
        match token.kind {
            TokenKind::Word(word) => Ok(word),
            found => Err(syntax(
                token.offset,
                format!("expected {expected}, found {found}"),
            )),
        }
    }

    fn take_string(&mut self, expected: &str) -> Result<String, ParseError> {
        let token = self.next().ok_or_else(|| {
            syntax(
                self.eof_offset,
                format!("expected {expected}, found end of input"),
            )
        })?;
        match token.kind {
            TokenKind::String(value) => Ok(value),
            found => Err(syntax(
                token.offset,
                format!("expected {expected}, found {found}"),
            )),
        }
    }

    fn take_number(&mut self, expected: &str) -> Result<u64, ParseError> {
        let token = self.next().ok_or_else(|| {
            syntax(
                self.eof_offset,
                format!("expected {expected}, found end of input"),
            )
        })?;
        match token.kind {
            TokenKind::Number(value) => Ok(value),
            found => Err(syntax(
                token.offset,
                format!("expected {expected}, found {found}"),
            )),
        }
    }

    fn check(&self, expected: &TokenKind) -> bool {
        self.tokens
            .get(self.cursor)
            .is_some_and(|token| &token.kind == expected)
    }

    fn current_offset(&self) -> usize {
        self.tokens
            .get(self.cursor)
            .map_or(self.eof_offset, |token| token.offset)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.cursor).cloned()?;
        self.cursor += 1;
        Some(token)
    }
}

#[cfg(test)]
mod tests {
    use crate::{AcceptanceCriterion, Artifact, SpecError, Topology};

    use super::*;

    #[test]
    fn parses_a_declarative_plasmid() {
        let source = r#"
            // implementation details are intentionally absent
            plasmid p_sensor {
                sequence "acgtacgt";
                topology circular;
                copies 2;
                acceptance {
                    exact_sequence;
                    minimum_concentration 100 ng_per_ul;
                    minimum_volume 20 ul;
                }
            }
        "#;

        let spec = parse(source).unwrap();
        assert_eq!(spec.name(), "p_sensor");
        assert_eq!(spec.copies().get(), 2);
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
    fn rejects_a_plan_without_verification_evidence() {
        let error = parse(
            r#"plasmid p_unverified {
                sequence "ACGT";
                acceptance { minimum_volume 20 ul; }
            }"#,
        )
        .unwrap_err();

        assert_eq!(
            error,
            ParseError::Specification(SpecError::MissingSequenceAcceptance)
        );
    }
}
