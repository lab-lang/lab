use std::fmt;
use std::num::NonZeroU16;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SpecError {
    #[error(
        "artifact name must be a non-empty identifier containing only letters, digits, or underscores"
    )]
    InvalidArtifactName,
    #[error("DNA sequence must not be empty")]
    EmptySequence,
    #[error("DNA sequence contains unsupported base '{base}' at offset {offset}")]
    UnsupportedBase { base: char, offset: usize },
    #[error("a plasmid must have circular topology")]
    PlasmidMustBeCircular,
    #[error("at least one physical copy must be requested")]
    ZeroCopies,
    #[error("the current plasmid pipeline requires an exact_sequence acceptance criterion")]
    MissingSequenceAcceptance,
}

/// An unambiguous DNA sequence normalized to uppercase.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct DnaSequence(String);

impl DnaSequence {
    pub fn new(sequence: impl AsRef<str>) -> Result<Self, SpecError> {
        let sequence = sequence.as_ref().to_ascii_uppercase();
        if sequence.is_empty() {
            return Err(SpecError::EmptySequence);
        }
        if let Some((offset, base)) = sequence
            .char_indices()
            .find(|(_, base)| !matches!(base, 'A' | 'C' | 'G' | 'T'))
        {
            return Err(SpecError::UnsupportedBase { base, offset });
        }
        Ok(Self(sequence))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for DnaSequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topology {
    Circular,
    Linear,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlasmidSpec {
    sequence: DnaSequence,
    topology: Topology,
}

impl PlasmidSpec {
    pub fn new(sequence: DnaSequence, topology: Topology) -> Result<Self, SpecError> {
        if topology != Topology::Circular {
            return Err(SpecError::PlasmidMustBeCircular);
        }
        Ok(Self { sequence, topology })
    }

    pub fn sequence(&self) -> &DnaSequence {
        &self.sequence
    }

    pub fn topology(&self) -> Topology {
        self.topology
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Concentration {
    nanograms_per_microliter: u32,
}

impl Concentration {
    pub const fn nanograms_per_microliter(value: u32) -> Self {
        Self {
            nanograms_per_microliter: value,
        }
    }

    pub const fn as_nanograms_per_microliter(self) -> u32 {
        self.nanograms_per_microliter
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Volume {
    microliters: u32,
}

impl Volume {
    pub const fn microliters(value: u32) -> Self {
        Self { microliters: value }
    }

    pub const fn as_microliters(self) -> u32 {
        self.microliters
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AcceptanceCriterion {
    ExactSequence,
    MinimumConcentration { concentration: Concentration },
    MinimumVolume { volume: Volume },
}

impl fmt::Display for AcceptanceCriterion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExactSequence => f.write_str("exact sequence identity"),
            Self::MinimumConcentration { concentration } => write!(
                f,
                "minimum concentration of {} ng/uL",
                concentration.as_nanograms_per_microliter()
            ),
            Self::MinimumVolume { volume } => {
                write!(f, "minimum volume of {} uL", volume.as_microliters())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Artifact {
    Plasmid(PlasmidSpec),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArtifactSpec {
    name: String,
    artifact: Artifact,
    copies: NonZeroU16,
    acceptance: Vec<AcceptanceCriterion>,
}

impl ArtifactSpec {
    pub fn plasmid(
        name: impl Into<String>,
        plasmid: PlasmidSpec,
        copies: u16,
        acceptance: Vec<AcceptanceCriterion>,
    ) -> Result<Self, SpecError> {
        let spec = Self {
            name: name.into(),
            artifact: Artifact::Plasmid(plasmid),
            copies: NonZeroU16::new(copies).ok_or(SpecError::ZeroCopies)?,
            acceptance,
        };
        spec.validate()?;
        Ok(spec)
    }

    pub fn validate(&self) -> Result<(), SpecError> {
        let mut chars = self.name.chars();
        let valid_start = chars
            .next()
            .is_some_and(|c| c == '_' || c.is_ascii_alphabetic());
        if !valid_start || !chars.all(|c| c == '_' || c.is_ascii_alphanumeric()) {
            return Err(SpecError::InvalidArtifactName);
        }
        if !self
            .acceptance
            .contains(&AcceptanceCriterion::ExactSequence)
        {
            return Err(SpecError::MissingSequenceAcceptance);
        }
        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }

    pub fn copies(&self) -> NonZeroU16 {
        self.copies
    }

    pub fn acceptance(&self) -> &[AcceptanceCriterion] {
        &self.acceptance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_validates_dna() {
        let sequence = DnaSequence::new("acgt").unwrap();
        assert_eq!(sequence.as_str(), "ACGT");
        assert_eq!(
            DnaSequence::new("ACNT"),
            Err(SpecError::UnsupportedBase {
                base: 'N',
                offset: 2
            })
        );
    }

    #[test]
    fn requires_verifiable_plasmid_specification() {
        let plasmid =
            PlasmidSpec::new(DnaSequence::new("ACGT").unwrap(), Topology::Circular).unwrap();
        let error = ArtifactSpec::plasmid("p_test", plasmid, 1, vec![]).unwrap_err();
        assert_eq!(error, SpecError::MissingSequenceAcceptance);
    }
}
