//! Whether a reviewed plan is being simulated or executed against physical Assets.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionMode {
    Simulation,
    Live,
}

impl ExecutionMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Simulation => "simulation",
            Self::Live => "live",
        }
    }
}
