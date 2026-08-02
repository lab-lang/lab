use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A laboratory capability that may be required by a compiler lowering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    DnaSynthesis,
    GibsonAssembly,
    GoldenGateAssembly,
    ChemicalTransformation,
    CultureIncubation,
    AntibioticSelection,
    CloneScreening,
    PlasmidPurification,
    SangerSequencing,
    DnaQuantification,
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::DnaSynthesis => "DNA synthesis",
            Self::GibsonAssembly => "Gibson assembly",
            Self::GoldenGateAssembly => "Golden Gate assembly",
            Self::ChemicalTransformation => "chemical transformation",
            Self::CultureIncubation => "culture incubation",
            Self::AntibioticSelection => "antibiotic selection",
            Self::CloneScreening => "clone screening",
            Self::PlasmidPurification => "plasmid purification",
            Self::SangerSequencing => "Sanger sequencing",
            Self::DnaQuantification => "DNA quantification",
        };
        f.write_str(name)
    }
}

/// An abstract DNA assembly strategy selected during compilation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyMethod {
    Gibson,
    GoldenGate,
}

impl AssemblyMethod {
    pub const fn required_capability(self) -> Capability {
        match self {
            Self::Gibson => Capability::GibsonAssembly,
            Self::GoldenGate => Capability::GoldenGateAssembly,
        }
    }
}

impl fmt::Display for AssemblyMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gibson => f.write_str("Gibson"),
            Self::GoldenGate => f.write_str("Golden Gate"),
        }
    }
}

/// The capabilities and policy preferences of a compilation target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LabProfile {
    name: String,
    capabilities: BTreeSet<Capability>,
    preferred_host: String,
    assembly_preference: Vec<AssemblyMethod>,
}

impl LabProfile {
    pub fn new(
        name: impl Into<String>,
        capabilities: impl IntoIterator<Item = Capability>,
        preferred_host: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            capabilities: capabilities.into_iter().collect(),
            preferred_host: preferred_host.into(),
            assembly_preference: vec![AssemblyMethod::Gibson, AssemblyMethod::GoldenGate],
        }
    }

    /// A deterministic local profile used by examples and tests.
    pub fn reference() -> Self {
        Self::new(
            "reference-lab",
            [
                Capability::DnaSynthesis,
                Capability::GibsonAssembly,
                Capability::ChemicalTransformation,
                Capability::CultureIncubation,
                Capability::AntibioticSelection,
                Capability::CloneScreening,
                Capability::PlasmidPurification,
                Capability::SangerSequencing,
                Capability::DnaQuantification,
            ],
            "DH5alpha",
        )
    }

    pub fn with_assembly_preference(
        mut self,
        preference: impl IntoIterator<Item = AssemblyMethod>,
    ) -> Self {
        self.assembly_preference = preference.into_iter().collect();
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn preferred_host(&self) -> &str {
        &self.preferred_host
    }

    pub fn capabilities(&self) -> &BTreeSet<Capability> {
        &self.capabilities
    }

    pub fn supports(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub fn assembly_preference(&self) -> &[AssemblyMethod] {
        &self.assembly_preference
    }
}
