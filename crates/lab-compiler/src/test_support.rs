//! The Golden Gate example package, compiled for the crate's unit tests.
//!
//! The example is a multi-module package, so its modules resolve each other
//! by package-qualified name rather than by path. Compiling them one at a
//! time into a shared environment, in dependency order, is what
//! `lab-project`'s package compiler does for a real build; these helpers do
//! the same thing over sources bundled at compile time so a backend test
//! needs no filesystem, manifest, or CLI.

use lab_language::{CheckedModule, ModuleId, SemanticEnvironment, compile_module_in_environment};

use crate::{PortableLairProgram, ProtocolLairProgram};

/// Every module of the example, ordered so each one's imports are already in
/// the environment when it compiles.
const MODULES: [(&str, &str); 6] = [
    (
        "golden_gate.designs.inventory",
        include_str!("../../../examples/golden-gate/src/designs/inventory.lab"),
    ),
    (
        "golden_gate.designs.plasmids",
        include_str!("../../../examples/golden-gate/src/designs/plasmids.lab"),
    ),
    (
        "golden_gate.designs.strains",
        include_str!("../../../examples/golden-gate/src/designs/strains.lab"),
    ),
    (
        "golden_gate.workflows.assemble",
        include_str!("../../../examples/golden-gate/src/workflows/assemble.lab"),
    ),
    (
        "golden_gate.workflows.build_strains",
        include_str!("../../../examples/golden-gate/src/workflows/build_strains.lab"),
    ),
    (
        "golden_gate.programs.reporter_panel",
        include_str!("../../../examples/golden-gate/src/programs/reporter_panel.lab"),
    ),
];

/// The example's checked modules: two composite plasmids, four strains, the
/// workflows that realize and transform them, and the program entry point.
pub fn golden_gate_modules() -> Vec<CheckedModule> {
    let mut environment = SemanticEnvironment::default();
    let mut modules = Vec::with_capacity(MODULES.len());
    for (name, source) in MODULES {
        let module = compile_module_in_environment(ModuleId::new(name), source, &environment)
            .unwrap_or_else(|error| panic!("{name} must compile: {error}"));
        environment.insert(name, module.interface.clone());
        modules.push(module);
    }
    modules
}

/// Those modules lowered together, the way a target build lowers the program
/// a package's entry point forms.
pub fn golden_gate_lair() -> PortableLairProgram {
    let modules = golden_gate_modules();
    let borrowed = modules.iter().collect::<Vec<_>>();
    PortableLairProgram::lower_program(&borrowed).expect("the example must lower")
}

/// The verified Protocol LAIR a backend compiles, with every Workflow
/// operation already replaced.
pub fn golden_gate_protocol() -> ProtocolLairProgram {
    golden_gate_lair()
        .select_protocol()
        .expect("the example must select a protocol")
}
