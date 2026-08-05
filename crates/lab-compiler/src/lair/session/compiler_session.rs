use pliron::builtin::ops::ModuleOp;
use pliron::combine::{Parser, eof};
use pliron::context::Context;
use pliron::irfmt::parsers::spaced;
use pliron::op::Op;
use pliron::operation::{Operation, verify_operation};
use pliron::parsable::parse_from_str;
use pliron::pass::{AnalysisManager, Pass};
use pliron::printable::Printable;

use crate::lair::dialect::detect_stage;
use crate::lair::pipeline::build_material_linearity_pass;
use crate::{IrStage, StageContract};

use super::{PassPipeline, RegisteredPass, SessionError, SessionOptions};

/// Owns the context, analyses, and one IR module for a compilation invocation.
pub struct CompilerSession {
    context: Context,
    module: Option<ModuleOp>,
    analyses: AnalysisManager,
    options: SessionOptions,
}

impl CompilerSession {
    pub fn new(options: SessionOptions) -> Self {
        let mut analyses = AnalysisManager::default();
        analyses.set_config(options.pass_manager_config());
        Self {
            context: Context::new(),
            module: None,
            analyses,
            options,
        }
    }

    /// Parse one complete textual Pliron module without implicitly verifying it.
    ///
    /// Parsing is transactional: failure leaves this session empty and usable.
    pub fn parse_ir(&mut self, source: &str) -> Result<(), SessionError> {
        self.require_empty()?;
        let mut context = Context::new();
        let root = parse_from_str(
            spaced(Operation::top_level_parser()).skip(eof()),
            &mut context,
            source,
        )
        .map_err(|error| SessionError::ParseIr(error.disp(&context).to_string()))?;
        let module = Operation::get_op::<ModuleOp>(root, &context).ok_or_else(|| {
            SessionError::ExpectedModule(Operation::get_opid(root, &context).to_string())
        })?;

        self.context = context;
        self.module = Some(module);
        self.reset_analyses();
        Ok(())
    }

    /// Apply generic Pliron verification to the entire module.
    pub fn verify(&self) -> Result<(), SessionError> {
        let module = self.module()?;
        verify_operation(module.get_operation(), &self.context).map_err(|error| {
            SessionError::VerificationFailed(error.disp(&self.context).to_string())
        })
    }

    /// Detect the current Lab Compiler stage after generic verification succeeds.
    pub fn detect_stage(&self) -> Result<IrStage, SessionError> {
        self.verify()?;
        detect_stage(&self.context, self.module()?).map_err(SessionError::StageContract)
    }

    /// Verify the generic IR invariants and one named Lab Compiler stage contract.
    pub fn verify_stage(&self, expected: IrStage) -> Result<(), SessionError> {
        self.verify()?;
        let actual =
            detect_stage(&self.context, self.module()?).map_err(SessionError::StageContract)?;
        StageContract::for_stage(expected)
            .verify(actual)
            .map_err(SessionError::StageContract)
    }

    /// Run a target-independent textual pass pipeline.
    pub fn run_pass_pipeline(&mut self, pipeline: &PassPipeline) -> Result<(), SessionError> {
        self.verify()?;
        for registered_pass in pipeline.passes() {
            let info = registered_pass.info();
            self.verify_stage(info.input)?;

            match registered_pass {
                RegisteredPass::CheckMaterialLinearity => {
                    self.execute_pass(&mut build_material_linearity_pass(), info.name)?
                }
            }
            self.verify_stage(info.output)?;
        }
        self.verify()
    }

    /// Print a complete, round-trippable textual representation of the module.
    pub fn ir(&self) -> Result<String, SessionError> {
        let module = self.module()?;
        Ok(module.get_operation().disp(&self.context).to_string())
    }

    fn execute_pass(&mut self, pass: &mut impl Pass, pass_name: &str) -> Result<(), SessionError> {
        let module = self.module()?;
        pass.run(
            module.get_operation(),
            &mut self.context,
            &mut self.analyses,
        )
        .map_err(|error| SessionError::PassFailed {
            name: pass_name.to_owned(),
            diagnostic: error.disp(&self.context).to_string(),
        })?;
        Ok(())
    }

    fn require_empty(&self) -> Result<(), SessionError> {
        if self.module.is_some() {
            Err(SessionError::ModuleAlreadyLoaded)
        } else {
            Ok(())
        }
    }

    fn module(&self) -> Result<ModuleOp, SessionError> {
        self.module.ok_or(SessionError::NoModule)
    }

    fn reset_analyses(&mut self) {
        self.analyses = AnalysisManager::default();
        self.analyses.set_config(self.options.pass_manager_config());
    }
}

impl Default for CompilerSession {
    fn default() -> Self {
        Self::new(SessionOptions::default())
    }
}
