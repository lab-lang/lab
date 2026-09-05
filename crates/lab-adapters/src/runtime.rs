//! Runtime composition for reviewed documents emitted or consumed by adapters.
//!
//! A runtime-capable adapter carries its exact document loader, optional simulator, and optional
//! live-executor factory in the same [`crate::AdapterRegistration`] used for planning and
//! lowering. The application composes that registration once; the execution path contains no
//! driver or document-format switch.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::net::SocketAddr;
use std::path::Path;

use anyhow::{Context, Result, bail};
use lab_capability::ProcedureImplementationId;
use lab_runtime::device_executors::ReviewedDocumentSimulationExecutor;
#[cfg(feature = "hardware")]
use lab_runtime::device_executors::{HamiltonStarExecutor, OdtcExecutor};
use lab_runtime::execution::{
    DocumentExecutor, ExecutorRegistry, LoadedExecutionAction, LoadedExecutionPlan,
    LoadedReviewedDocument, ReviewedDocumentLoadRequest, ReviewedDocumentLoaderRegistry,
};

use crate::backend::hamilton::star::StarAdapterProfile;
use crate::{AdapterRegistration, AdapterRegistry};

/// An exact parser and semantic validator for one reviewed document format.
pub type ReviewedDocumentLoader =
    for<'a> fn(ReviewedDocumentLoadRequest<'a>) -> Result<LoadedReviewedDocument>;

/// Constructs a fresh no-hardware executor for one exact reviewed document binding.
pub type SimulationExecutorFactory = fn() -> Box<dyn DocumentExecutor>;

/// Constructs the stateful live factory for one exact adapter/document pair.
pub type LiveExecutorFactoryConstructor = fn() -> Box<dyn LiveExecutorFactory>;

/// The runtime half of one exact reviewed document contract.
#[derive(Clone)]
pub struct RuntimeDocumentRegistration {
    implementation: ProcedureImplementationId,
    format: String,
    loader: ReviewedDocumentLoader,
    simulation: Option<SimulationExecutorFactory>,
    live: Option<LiveExecutorFactoryConstructor>,
}

impl fmt::Debug for RuntimeDocumentRegistration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeDocumentRegistration")
            .field("implementation", &self.implementation)
            .field("format", &self.format)
            .field("simulation", &self.simulation.is_some())
            .field("live", &self.live.is_some())
            .finish_non_exhaustive()
    }
}

impl RuntimeDocumentRegistration {
    pub fn new(
        implementation: ProcedureImplementationId,
        format: impl Into<String>,
        loader: ReviewedDocumentLoader,
    ) -> Self {
        Self {
            implementation,
            format: format.into(),
            loader,
            simulation: None,
            live: None,
        }
    }

    pub fn with_simulation(mut self, factory: SimulationExecutorFactory) -> Self {
        self.simulation = Some(factory);
        self
    }

    pub fn with_live_executor(mut self, factory: LiveExecutorFactoryConstructor) -> Self {
        self.live = Some(factory);
        self
    }

    pub fn format(&self) -> &str {
        &self.format
    }

    pub fn implementation(&self) -> &ProcedureImplementationId {
        &self.implementation
    }

    pub fn simulation_factory(&self) -> Option<SimulationExecutorFactory> {
        self.simulation
    }

    pub fn live_executor_factory(&self) -> Option<LiveExecutorFactoryConstructor> {
        self.live
    }
}

/// Immutable inputs for constructing one live executor from a frozen reviewed plan.
#[derive(Clone, Copy, Debug)]
pub struct LiveExecutorFactoryRequest<'a> {
    pub execution_directory: &'a Path,
    pub asset: &'a str,
    pub profile_path: &'a str,
    pub endpoint: Option<SocketAddr>,
}

/// A live executor plus whether its factory consumed the CLI-provided network endpoint.
pub struct BuiltLiveExecutor {
    pub executor: Box<dyn DocumentExecutor>,
    pub consumed_endpoint: bool,
}

/// Stateful constructor for executors of one exact adapter/document pair.
///
/// The factory lives for the complete registry build, so an integration can enforce process-level
/// constraints such as a USB transport that addresses only one physical device.
pub trait LiveExecutorFactory: Send {
    fn build(&mut self, request: LiveExecutorFactoryRequest<'_>) -> Result<BuiltLiveExecutor>;
}

/// Optional runtime records layered onto the dependency-light adapter registration.
pub trait AdapterRuntimeRegistrationExt: Sized {
    /// Carries one reviewed-document integration with this adapter registration.
    fn with_runtime_document(self, document: RuntimeDocumentRegistration) -> Self;

    /// Exact reviewed-document integrations contributed by this adapter.
    fn runtime_documents(&self) -> impl Iterator<Item = &RuntimeDocumentRegistration>;
}

impl AdapterRuntimeRegistrationExt for AdapterRegistration {
    fn with_runtime_document(self, document: RuntimeDocumentRegistration) -> Self {
        self.with_extension(document)
    }

    fn runtime_documents(&self) -> impl Iterator<Item = &RuntimeDocumentRegistration> {
        self.extensions::<RuntimeDocumentRegistration>()
    }
}

/// Optional runtime services composed over the core planning/lowering registry.
///
/// Adapter crates that only plan and emit artifacts never need `lab-runtime`. Runtime-capable
/// applications import this trait from `lab-adapters` to load or execute reviewed documents.
pub trait AdapterRuntimeRegistryExt {
    /// Builds the exact loader set contributed by this application composition.
    fn reviewed_document_loaders(&self) -> Result<ReviewedDocumentLoaderRegistry>;

    /// Builds simulation executors for the exact bindings present in a reviewed plan.
    fn simulation_executors(&self, loaded: &LoadedExecutionPlan) -> Result<ExecutorRegistry>;

    /// Builds live executors for every exact adapter/document binding in a reviewed plan.
    fn live_executors(
        &self,
        loaded: &LoadedExecutionPlan,
        endpoints: &BTreeMap<String, SocketAddr>,
    ) -> Result<ExecutorRegistry>;
}

impl AdapterRuntimeRegistryExt for AdapterRegistry {
    fn reviewed_document_loaders(&self) -> Result<ReviewedDocumentLoaderRegistry> {
        validate_runtime_registry(self).map_err(anyhow::Error::msg)?;
        let mut loaders = ReviewedDocumentLoaderRegistry::new();
        for registration in self.registrations() {
            for document in registration.runtime_documents() {
                loaders.register(
                    registration.descriptor.id.clone(),
                    document.implementation.to_string(),
                    document.format.clone(),
                    document.loader,
                )?;
            }
        }
        Ok(loaders)
    }

    fn simulation_executors(&self, loaded: &LoadedExecutionPlan) -> Result<ExecutorRegistry> {
        validate_runtime_registry(self).map_err(anyhow::Error::msg)?;
        let mut keys = BTreeSet::new();
        let mut executors = ExecutorRegistry::new();
        for node in &loaded.nodes {
            let LoadedExecutionAction::Execute {
                requirements,
                document: Some(document),
            } = &node.action
            else {
                continue;
            };
            let requirement = requirements
                .first()
                .context("execute nodes require at least one frozen requirement binding")?;
            let adapter = requirement
                .adapter
                .as_ref()
                .context("simulation requires a frozen adapter binding")?;
            let registration = self.registration(&adapter.driver).with_context(|| {
                format!(
                    "adapter '{}' is not present in this application composition",
                    adapter.driver
                )
            })?;
            let implementation_id = requirement
                .procedure_implementation
                .as_deref()
                .context("simulation requires an exact Procedure implementation binding")?;
            let implementation = registration
                .descriptor
                .procedure_implementations
                .iter()
                .find(|implementation| implementation.id.as_str() == implementation_id)
                .with_context(|| {
                    format!(
                        "adapter '{}' does not provide Procedure implementation '{}'",
                        adapter.driver, implementation_id
                    )
                })?;
            if !implementation.services.simulation {
                bail!(
                    "Procedure implementation '{}' does not provide simulation",
                    implementation_id
                );
            }
            for requirement in requirements {
                if requirement.procedure_implementation.as_deref() != Some(implementation_id) {
                    bail!(
                        "one reviewed document cannot combine Procedure implementations '{}' and '{}'",
                        implementation_id,
                        requirement
                            .procedure_implementation
                            .as_deref()
                            .unwrap_or("none")
                    );
                }
                if !implementation
                    .capability_kinds
                    .iter()
                    .any(|kind| kind.as_str() == requirement.capability_kind)
                {
                    bail!(
                        "adapter '{}' does not simulate capability '{}'",
                        adapter.driver,
                        requirement.capability_kind
                    );
                }
                if !implementation
                    .control_modes
                    .iter()
                    .any(|mode| mode.iri() == requirement.control_mode)
                {
                    bail!(
                        "adapter '{}' does not accept control mode '{}'",
                        adapter.driver,
                        requirement.control_mode
                    );
                }
            }
            let runtime = runtime_document(registration, implementation_id, document.format())
                .with_context(|| {
                format!(
                    "adapter '{}' implementation '{}' has no runtime registration for reviewed format '{}'",
                    adapter.driver,
                    implementation_id,
                    document.format()
                )
            })?;
            let factory = runtime.simulation.with_context(|| {
                format!(
                    "adapter '{}' does not simulate reviewed format '{}'",
                    adapter.driver,
                    document.format()
                )
            })?;
            let key = (
                requirement.asset.clone(),
                adapter.driver.clone(),
                implementation_id.to_owned(),
                document.format().to_owned(),
            );
            if keys.insert(key.clone()) {
                executors.register(key.0, key.1, key.2, key.3, factory())?;
            }
        }
        Ok(executors)
    }

    fn live_executors(
        &self,
        loaded: &LoadedExecutionPlan,
        endpoints: &BTreeMap<String, SocketAddr>,
    ) -> Result<ExecutorRegistry> {
        let mut bindings = BTreeMap::<(String, String, String, String), (String, String)>::new();
        for node in &loaded.nodes {
            let LoadedExecutionAction::Execute {
                requirements,
                document: Some(document),
            } = &node.action
            else {
                continue;
            };
            let requirement = requirements
                .first()
                .context("execute nodes require at least one frozen requirement binding")?;
            let Some(adapter) = &requirement.adapter else {
                continue;
            };
            let implementation = requirement
                .procedure_implementation
                .as_deref()
                .context("live execution requires an exact Procedure implementation binding")?;
            if requirements.iter().any(|requirement| {
                requirement.procedure_implementation.as_deref() != Some(implementation)
            }) {
                bail!("one reviewed document cannot combine different Procedure implementations");
            }
            let key = (
                requirement.asset.clone(),
                adapter.driver.clone(),
                implementation.to_owned(),
                document.format().to_owned(),
            );
            let profile = (adapter.profile_path.clone(), adapter.profile_sha256.clone());
            if let Some(prior) = bindings.insert(key.clone(), profile.clone())
                && prior != profile
            {
                bail!(
                    "asset '{}' uses adapter '{}' implementation '{}' and format '{}' with two different frozen profiles",
                    key.0,
                    key.1,
                    key.2,
                    key.3
                );
            }
        }

        validate_runtime_registry(self).map_err(anyhow::Error::msg)?;
        let mut factories = live_executor_factories(self)?;
        let mut used_endpoints = BTreeSet::new();
        let mut executors = ExecutorRegistry::new();
        for ((asset, driver, implementation, format), (profile_path, _profile_sha256)) in bindings {
            let built = factories.build(
                &driver,
                &implementation,
                &format,
                LiveExecutorFactoryRequest {
                    execution_directory: &loaded.directory,
                    asset: &asset,
                    profile_path: &profile_path,
                    endpoint: endpoints.get(&asset).copied(),
                },
            )?;
            if built.consumed_endpoint {
                used_endpoints.insert(asset.clone());
            }
            executors.register(&asset, &driver, &implementation, &format, built.executor)?;
        }
        if let Some(unused) = endpoints
            .keys()
            .find(|asset| !used_endpoints.contains(*asset))
        {
            bail!(
                "--asset-endpoint supplies an address for Asset '{unused}', which the reviewed plan does not use as a networked executor"
            );
        }
        Ok(executors)
    }
}

fn live_executor_factories(registry: &AdapterRegistry) -> Result<LiveExecutorFactoryRegistry> {
    let mut factories = LiveExecutorFactoryRegistry::default();
    for registration in registry.registrations() {
        for document in registration.runtime_documents() {
            if let Some(factory) = document.live {
                factories.register(
                    registration.descriptor.id.clone(),
                    document.implementation.to_string(),
                    document.format.clone(),
                    factory(),
                )?;
            }
        }
    }
    Ok(factories)
}

pub(crate) fn validate_runtime_registry(
    registry: &AdapterRegistry,
) -> std::result::Result<(), String> {
    for registration in registry.registrations() {
        validate_runtime_documents(registration)?;
    }
    Ok(())
}

pub(crate) fn validate_runtime_documents(
    registration: &AdapterRegistration,
) -> std::result::Result<(), String> {
    let descriptor = &registration.descriptor;
    let mut bindings = BTreeSet::new();
    for document in registration.runtime_documents() {
        if document.format.is_empty() {
            return Err(format!(
                "adapter '{}' has a runtime document with an empty format",
                descriptor.id
            ));
        }
        if !bindings.insert((document.implementation.clone(), document.format.clone())) {
            return Err(format!(
                "adapter '{}' registers implementation '{}' and reviewed format '{}' more than once",
                descriptor.id, document.implementation, document.format
            ));
        }
        let implementation = descriptor
            .procedure_implementations
            .iter()
            .find(|implementation| implementation.id == document.implementation)
            .ok_or_else(|| {
                format!(
                    "adapter '{}' runtime document references unknown Procedure implementation '{}'",
                    descriptor.id, document.implementation
                )
            })?;
        if !implementation
            .accepted_run_formats
            .contains(&document.format)
            && !implementation
                .emitted_run_formats
                .contains(&document.format)
        {
            return Err(format!(
                "adapter '{}' implementation '{}' registers reviewed format '{}' but neither accepts nor emits it",
                descriptor.id, document.implementation, document.format
            ));
        }
        if document.simulation.is_some() && !implementation.services.simulation {
            return Err(format!(
                "adapter '{}' registers a simulator but does not declare simulation service",
                descriptor.id
            ));
        }
        if document.live.is_some() && !implementation.services.runtime {
            return Err(format!(
                "adapter '{}' registers a live executor but does not declare runtime service",
                descriptor.id
            ));
        }
    }
    for implementation in &descriptor.procedure_implementations {
        if implementation.services.simulation
            && !registration.runtime_documents().any(|document| {
                document.implementation == implementation.id && document.simulation.is_some()
            })
        {
            return Err(format!(
                "adapter '{}' implementation '{}' declares simulation service without a simulator registration",
                descriptor.id, implementation.id
            ));
        }
        if implementation.services.runtime
            && !registration.runtime_documents().any(|document| {
                document.implementation == implementation.id && document.live.is_some()
            })
        {
            return Err(format!(
                "adapter '{}' implementation '{}' declares runtime service without a live executor registration",
                descriptor.id, implementation.id
            ));
        }
    }
    Ok(())
}

fn runtime_document<'a>(
    registration: &'a AdapterRegistration,
    implementation: &str,
    format: &str,
) -> Option<&'a RuntimeDocumentRegistration> {
    registration.runtime_documents().find(|document| {
        document.implementation.as_str() == implementation && document.format == format
    })
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LiveExecutorFactoryKey {
    adapter_id: String,
    implementation: String,
    format: String,
}

#[derive(Default)]
struct LiveExecutorFactoryRegistry {
    factories: BTreeMap<LiveExecutorFactoryKey, Box<dyn LiveExecutorFactory>>,
}

impl LiveExecutorFactoryRegistry {
    fn register(
        &mut self,
        adapter_id: impl Into<String>,
        implementation: impl Into<String>,
        format: impl Into<String>,
        factory: Box<dyn LiveExecutorFactory>,
    ) -> Result<()> {
        let key = LiveExecutorFactoryKey {
            adapter_id: adapter_id.into(),
            implementation: implementation.into(),
            format: format.into(),
        };
        if self.factories.insert(key.clone(), factory).is_some() {
            bail!(
                "a live executor factory is already registered for adapter '{}' implementation '{}' and format '{}'",
                key.adapter_id,
                key.implementation,
                key.format
            );
        }
        Ok(())
    }

    fn build(
        &mut self,
        adapter_id: &str,
        implementation: &str,
        format: &str,
        request: LiveExecutorFactoryRequest<'_>,
    ) -> Result<BuiltLiveExecutor> {
        self.factories
            .get_mut(&LiveExecutorFactoryKey {
                adapter_id: adapter_id.to_owned(),
                implementation: implementation.to_owned(),
                format: format.to_owned(),
            })
            .with_context(|| {
                format!(
                    "this Lab runtime has no live executor factory for adapter '{adapter_id}', implementation '{implementation}', and format '{format}'"
                )
            })?
            .build(request)
    }
}

pub(crate) fn simulation_executor() -> Box<dyn DocumentExecutor> {
    Box::<ReviewedDocumentSimulationExecutor>::default()
}

pub(crate) fn hamilton_star_live_factory() -> Box<dyn LiveExecutorFactory> {
    Box::<HamiltonStarExecutorFactory>::default()
}

pub(crate) fn odtc_live_factory() -> Box<dyn LiveExecutorFactory> {
    Box::new(OdtcExecutorFactory)
}

#[derive(Default)]
struct HamiltonStarExecutorFactory {
    bound_asset: Option<String>,
}

impl LiveExecutorFactory for HamiltonStarExecutorFactory {
    fn build(&mut self, request: LiveExecutorFactoryRequest<'_>) -> Result<BuiltLiveExecutor> {
        if let Some(bound) = &self.bound_asset
            && bound != request.asset
        {
            bail!(
                "this runtime can address only one Hamilton STAR over USB, but the reviewed plan binds '{}' and '{}'",
                bound,
                request.asset
            );
        }
        let path = request.execution_directory.join(request.profile_path);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .context("a STAR adapter profile needs a UTF-8 file name")?;
        let profile = StarAdapterProfile::parse(name, &text)
            .with_context(|| format!("failed to parse frozen profile {}", path.display()))?;
        self.bound_asset = Some(request.asset.to_owned());

        #[cfg(feature = "hardware")]
        {
            Ok(BuiltLiveExecutor {
                executor: Box::new(HamiltonStarExecutor::new(
                    request.asset,
                    profile.run.autoload_park_track,
                )),
                consumed_endpoint: false,
            })
        }
        #[cfg(not(feature = "hardware"))]
        {
            let _ = profile;
            bail!(
                "the Lab application was built without live hardware support for adapter 'hamilton.star'"
            )
        }
    }
}

struct OdtcExecutorFactory;

impl LiveExecutorFactory for OdtcExecutorFactory {
    fn build(&mut self, request: LiveExecutorFactoryRequest<'_>) -> Result<BuiltLiveExecutor> {
        let address = request.endpoint.with_context(|| {
            format!(
                "Inheco ODTC Asset '{}' has no runtime address; pass --asset-endpoint '{}=<ip:port>'",
                request.asset, request.asset
            )
        })?;
        #[cfg(feature = "hardware")]
        {
            Ok(BuiltLiveExecutor {
                executor: Box::new(OdtcExecutor::new(request.asset, address)),
                consumed_endpoint: true,
            })
        }
        #[cfg(not(feature = "hardware"))]
        {
            let _ = address;
            bail!(
                "the Lab application was built without live hardware support for adapter 'inheco.odtc'"
            )
        }
    }
}
