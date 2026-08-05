use super::BackendCompilation;
use crate::CheckedModule;
use crate::backend::Backend;

#[derive(Clone, Copy, Debug, Default)]
pub struct Compiler;

impl Compiler {
    pub fn compile_backend<B>(
        &self,
        module: &CheckedModule,
        backend: &B,
    ) -> Result<BackendCompilation<B::Program>, B::Error>
    where
        B: Backend<CheckedModule>,
    {
        let descriptor = backend.descriptor();
        let program = backend.compile(module)?;
        Ok(BackendCompilation::new(descriptor, program))
    }
}
