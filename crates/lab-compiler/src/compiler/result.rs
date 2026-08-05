use crate::backend::BackendDescriptor;

/// Typed result of compiling a checked Lab module with a concrete backend.
pub struct BackendCompilation<Program> {
    descriptor: BackendDescriptor,
    program: Program,
}

impl<Program> BackendCompilation<Program> {
    pub(crate) fn new(descriptor: BackendDescriptor, program: Program) -> Self {
        Self {
            descriptor,
            program,
        }
    }

    pub fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    pub fn program(&self) -> &Program {
        &self.program
    }

    pub fn into_program(self) -> Program {
        self.program
    }
}
