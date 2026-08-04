use crate::ExecutablePlan;

/// The result of compilation: verified target-selected LAIR text and a backend-neutral plan.
///
/// The compiler's underlying IR framework is deliberately not part of this stable result API.
pub struct Compilation {
    ir: String,
    plan: ExecutablePlan,
}

impl Compilation {
    pub(crate) fn new(ir: String, plan: ExecutablePlan) -> Self {
        Self { ir, plan }
    }

    pub fn plan(&self) -> &ExecutablePlan {
        &self.plan
    }

    /// Return the complete, round-trippable textual LAIR produced by compilation.
    pub fn ir(&self) -> String {
        self.ir.clone()
    }
}
