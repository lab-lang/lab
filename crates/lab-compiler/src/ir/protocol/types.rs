use pliron::context::Context;
use pliron::derive::pliron_type;
use pliron::r#type::{Type, TypeHandle};

/// The physical state of a material value in the Protocol dialect.
#[pliron_type(name = "protocol.material", format, verifier = "succ")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialType {
    CompetentCells,
    LinearDna,
    CircularDna,
    TransformedCulture,
    RecoveredCulture,
    ColonyPool,
    SelectedClone,
    CloneCulture,
    PurifiedPlasmid,
    AssayAliquot,
    ValidatedPlasmid,
}

impl MaterialType {
    pub fn get(self, ctx: &Context) -> TypeHandle {
        Self::instantiate(self, ctx).into()
    }
}

/// Evidence produced by an assay. Evidence is information and is not affine.
#[pliron_type(name = "protocol.evidence", format, verifier = "succ")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvidenceType {
    SequenceIdentity,
    Concentration,
    Volume,
}

impl EvidenceType {
    pub fn get(self, ctx: &Context) -> TypeHandle {
        Self::instantiate(self, ctx).into()
    }
}
