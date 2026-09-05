// Construction APIs are consumed by LAIR transformations, which may be supplied
// independently of the compiler's source frontend.

use pliron::builtin::attributes::StringAttr;
use pliron::builtin::op_interfaces::NOpdsInterface;
use pliron::common_traits::Verify;
use pliron::context::Context;
use pliron::derive::{pliron_op, pliron_type};
use pliron::op::Op;
use pliron::operation::Operation;
use pliron::result::Result;
use pliron::verify_err;

use crate::design::ArtifactDesign;
use crate::ir::attributes::require_string;

/// A declarative biological artifact design. Design values are freely reusable.
#[pliron_type(
    name = "design.artifact",
    format,
    generate_get = true,
    verifier = "succ"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DesignType;

#[pliron_op(
    name = "design.define",
    format,
    attributes = (
        defined_artifact_name: StringAttr,
        defined_artifact_document: StringAttr
    ),
    interfaces = [NOpdsInterface<0>],
    results = (design: DesignType)
)]
/// Preserve one checked artifact declaration without interpreting package
/// vocabulary in the compiler core.
pub struct DesignArtifactOp;

impl DesignArtifactOp {
    pub fn new(ctx: &mut Context, design: &ArtifactDesign) -> Self {
        let op = Operation::new(
            ctx,
            Self::get_concrete_op_info(),
            vec![DesignType::get(ctx).into()],
            vec![],
            vec![],
            0,
        );
        let result = Self { op };
        result.set_attr_defined_artifact_name(ctx, StringAttr::new(design.name.clone()));
        result.set_attr_defined_artifact_document(
            ctx,
            StringAttr::new(
                serde_json::to_string(design)
                    .expect("checked artifact design documents serialize infallibly"),
            ),
        );
        result
    }
}

impl Verify for DesignArtifactOp {
    fn verify(&self, ctx: &Context) -> Result<()> {
        require_string(
            self.get_attr_defined_artifact_name(ctx).as_deref(),
            "design.define artifact_name",
            self.loc(ctx),
        )?;
        let Some(document) = self.get_attr_defined_artifact_document(ctx) else {
            return verify_err!(
                self.loc(ctx),
                "design.define is missing defined_artifact_document"
            );
        };
        let design =
            serde_json::from_str::<ArtifactDesign>(document.as_str()).map_err(|error| {
                pliron::input_error!(
                    self.loc(ctx),
                    "design.define artifact_document is invalid: {error}"
                )
            })?;
        if let Err(error) = design.validate() {
            return verify_err!(
                self.loc(ctx),
                "design.define artifact_document is invalid: {error}"
            );
        }
        if self
            .get_attr_defined_artifact_name(ctx)
            .is_none_or(|name| name.as_str() != design.name)
        {
            return verify_err!(
                self.loc(ctx),
                "design.define artifact_name must match artifact_document"
            );
        }
        Ok(())
    }
}
