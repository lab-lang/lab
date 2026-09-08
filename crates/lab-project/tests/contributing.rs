use lab_project::{ProjectCompilation, application_extensions};
use serde_json::Value;

#[test]
fn a_facade_only_adapter_lowers_complete_allocated_programs() {
    use lab_adapter_api::{AdapterInvocation, InvocationAdapter, adapter_invocation_id};
    use lab_project::ProjectPlanningRequest;
    use std::collections::{BTreeMap, BTreeSet};
    let extension = lab_example_preview_adapter::registration().unwrap();
    let implementation = extension.descriptor.procedure_implementations[0].clone();
    let extensions = application_extensions().unwrap();
    let registry = extensions.adapters.with_registration(extension).unwrap();
    let profile = registry
        .validate_profile(lab_example_preview_adapter::DRIVER, "preview", "")
        .unwrap();
    let application = ProjectCompilation::load_with_extensions(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/golden-gate"),
        registry,
        extensions.procedures,
    )
    .unwrap();
    let mut plan = application
        .plan(ProjectPlanningRequest::default())
        .unwrap()
        .adapter_invocations;
    let adapter = InvocationAdapter {
        driver: profile.driver.clone(),
        profile_path: "preview.toml".into(),
        profile_sha256: profile.sha256.clone(),
        features: BTreeSet::new(),
        accepted_run_formats: implementation.accepted_run_formats.clone(),
        emitted_run_formats: implementation.emitted_run_formats.clone(),
    };
    // Feed the example a complete allocation fixture. This tests its file-lowering
    // boundary; the preview deliberately has no physical instrument qualification.
    for task in plan
        .allocated
        .methods
        .iter_mut()
        .flat_map(|method| &mut method.tasks)
    {
        if task
            .program
            .as_ref()
            .is_some_and(|p| p.contract == implementation.contract)
        {
            for requirement in &mut task.requirements {
                if requirement.adapter.is_some() {
                    requirement.adapter = Some(adapter.clone());
                    requirement.procedure_implementation = Some(implementation.id.clone());
                }
            }
        }
    }
    let mut invocations = BTreeMap::<String, AdapterInvocation>::new();
    for task in plan
        .allocated
        .methods
        .iter()
        .flat_map(|method| &method.tasks)
    {
        for requirement in &task.requirements {
            if let Some(adapter) = &requirement.adapter {
                let id = adapter_invocation_id(&requirement.asset, adapter);
                let invocation =
                    invocations
                        .entry(id.clone())
                        .or_insert_with(|| AdapterInvocation {
                            id,
                            asset: requirement.asset.clone(),
                            adapter: adapter.clone(),
                            tasks: Vec::new(),
                            requirements: Vec::new(),
                        });
                if !invocation.tasks.contains(&task.id) {
                    invocation.tasks.push(task.id.clone());
                }
                invocation.requirements.push(requirement.id.clone());
            }
        }
    }
    plan.invocations = invocations.into_values().collect();
    for invocation in &mut plan.invocations {
        invocation.tasks.sort();
        invocation.requirements.sort();
    }
    plan.validate(application.procedures().contracts()).unwrap();
    let mut count = 0;
    for invocation in plan
        .invocations
        .iter()
        .filter(|i| i.adapter.driver == profile.driver)
    {
        let lowered = application
            .adapters()
            .lower_invocation(
                &profile,
                &plan,
                invocation,
                application.procedures().contracts(),
            )
            .unwrap();
        assert_eq!(lowered.documents.len(), invocation.tasks.len());
        for document in lowered.documents {
            let file = lowered.artifacts.get(&document.path).unwrap();
            let emitted: Value = serde_json::from_slice(file.contents()).unwrap();
            let task = plan
                .allocated
                .methods
                .iter()
                .flat_map(|m| &m.tasks)
                .find(|t| emitted["task"] == t.id.as_str())
                .unwrap();
            assert_eq!(
                emitted["program"],
                serde_json::to_value(&task.program).unwrap()
            );
            assert_eq!(
                emitted["materials"],
                serde_json::to_value(&task.materials).unwrap()
            );
            count += 1;
        }
    }
    assert!(
        count >= 3,
        "assembly, transformation, and dilution reach the same adapter"
    );
}

fn project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for (path, contents) in [
        (
            "lab.toml",
            include_str!("../../../examples/contributing/scientific-package/lab.toml"),
        ),
        (
            "src/main.lab",
            include_str!("../../../examples/contributing/scientific-package/src/main.lab"),
        ),
        (
            "src/science.lab",
            include_str!("../../../examples/contributing/scientific-package/src/science.lab"),
        ),
        (
            "methods/homogenize.json",
            include_str!(
                "../../../examples/contributing/scientific-package/methods/homogenize.json"
            ),
        ),
    ] {
        let destination = root.path().join(path);
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(destination, contents).unwrap();
    }
    root
}

fn refine(compilation: &ProjectCompilation) -> lab_compiler::planning::PlanningProblem {
    let methods = compilation.compose_methods([], true).unwrap();
    let mut problem = compilation
        .refine_modules(&[], "contribution_example.main", &methods)
        .unwrap()
        .planning_problem;
    // Templates may omit default fields; compare decoded canonical semantics and
    // every derived capability, port, and material requirement.
    for task in problem
        .choices
        .iter_mut()
        .flat_map(|c| &mut c.candidates)
        .flat_map(|c| &mut c.tasks)
    {
        if let Some(program) = &mut task.program {
            *program = lab_compiler::procedure::ProcedureProgram::from_pipetting(
                &program
                    .validate(compilation.procedures().contracts())
                    .unwrap()
                    .pipetting()
                    .unwrap(),
            );
        }
    }
    problem
}

#[test]
fn a_public_rust_extension_and_a_package_template_produce_the_same_program() {
    let root = project();
    let template = refine(&ProjectCompilation::load(root.path()).unwrap());
    let path = root.path().join("methods/homogenize.json");
    let mut catalog: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let execution = &mut catalog["methods"][0]["tasks"][0]["execution"];
    execution["kind"] = "builder".into();
    execution["builder"] = lab_example_pipetting::BUILDER.into();
    execution.as_object_mut().unwrap().remove("body");
    std::fs::write(&path, serde_json::to_string_pretty(&catalog).unwrap()).unwrap();

    // Registration is explicit and is validated before a package is accepted.
    let error = ProjectCompilation::load(root.path()).unwrap_err();
    assert!(format!("{error:?}").contains(lab_example_pipetting::BUILDER));
    let extensions = application_extensions().unwrap();
    let compilation = ProjectCompilation::load_with_extensions(
        root.path(),
        extensions.adapters,
        extensions
            .procedures
            .with_builder(lab_example_pipetting::registration())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(refine(&compilation), template);

    // An authoring error reaches the public callback as a checked diagnostic.
    catalog["methods"][0]["tasks"][0]["parameters"][1]["value"]["value"]["value"]["unit"] =
        "http://qudt.org/vocab/unit/SEC".into();
    std::fs::write(path, serde_json::to_string_pretty(&catalog).unwrap()).unwrap();
    let compilation = ProjectCompilation::load_with_extensions(
        root.path(),
        compilation.adapters().clone(),
        compilation.procedures().clone(),
    )
    .unwrap();
    let error = compilation
        .refine_modules(
            &[],
            "contribution_example.main",
            &compilation.compose_methods([], true).unwrap(),
        )
        .unwrap_err();
    assert!(format!("{error:?}").contains("mix_volume"));
}
