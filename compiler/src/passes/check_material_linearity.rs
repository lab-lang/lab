use pliron::context::{Context, Ptr};
use pliron::operation::Operation;
use pliron::pass::{AnalysisManager, Pass, PassResult};
use pliron::result::Result;

use crate::analyses::MaterialLinearityAnalysis;

/// Require every physical Protocol material value to have at most one consumer.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CheckMaterialLinearityPass;

impl Pass for CheckMaterialLinearityPass {
    fn name(&self) -> &str {
        "protocol-check-material-linearity"
    }

    fn run(
        &mut self,
        operation: Ptr<Operation>,
        context: &mut Context,
        analyses: &mut AnalysisManager,
    ) -> Result<PassResult> {
        let _linearity = analyses.get_analysis::<MaterialLinearityAnalysis>(operation, context)?;
        Ok(PassResult::default())
    }
}
