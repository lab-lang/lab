//! LAIR Protocol dialect for method-selected biological procedures and evidence.

use pliron::context::Context;
use pliron::derive::{pliron_attr, pliron_type};
use pliron::r#type::{Type, TypeHandle};

mod manufacturing;
mod validation;
mod verification;

/// A DNA assembly strategy selected independently of any facility.
#[pliron_attr(name = "protocol.assembly_method", format, verifier = "succ")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AssemblyMethodAttr {
    Gibson,
    GoldenGate,
}

/// The physical state of a material value in the Protocol dialect.
#[pliron_type(name = "protocol.material", format, verifier = "succ")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaterialType {
    CompetentCells,
    LinearDna,
    CircularDna,
    EngineeredStrain,
    TransformedCulture,
    RecoveredCulture,
    DilutedCulture,
    SelectionPlate,
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

pub(crate) use manufacturing::{
    AssembleOp, DiluteOp, PlateOp, ProvisionOp, RecoverOp, SynthesizeOp, TransformOp,
};
