//! Absolute capability-kind IRIs used by bundled durable action contracts.
//!
//! SBOLInventory capability kinds are an open vocabulary. Constants already present in Profile
//! 0.2 use its normative names; the remaining operation-specific terms use the same namespace and
//! are explicit extension terms pending inclusion in a future vocabulary snapshot.

pub(crate) const ARTIFACT_REALIZATION: &str = "https://sbol.io/ns/capability#ArtifactRealization";
pub(crate) const PLATE_IMAGING: &str = "https://sbol.io/ns/capability#PlateImaging";
pub(crate) const DNA_SYNTHESIS: &str = "https://sbol.io/ns/capability#DnaSynthesis";
pub(crate) const DNA_ASSEMBLY: &str = "https://sbol.io/ns/capability#DnaAssembly";
pub(crate) const MATERIAL_PROVISIONING: &str = "https://sbol.io/ns/capability#MaterialProvisioning";
pub(crate) const CHEMICAL_TRANSFORMATION: &str =
    "https://sbol.io/ns/capability#ChemicalTransformation";
pub(crate) const INCUBATION: &str = "https://sbol.io/ns/capability#Incubation";
pub(crate) const LIQUID_HANDLING: &str = "https://sbol.io/ns/capability#LiquidHandling";
pub(crate) const ANTIBIOTIC_SELECTION: &str = "https://sbol.io/ns/capability#AntibioticSelection";
pub(crate) const COLONY_PICKING: &str = "https://sbol.io/ns/capability#ColonyPicking";
pub(crate) const CLONE_SCREENING: &str = "https://sbol.io/ns/capability#CloneScreening";
pub(crate) const PLASMID_PURIFICATION: &str = "https://sbol.io/ns/capability#PlasmidPurification";
pub(crate) const SANGER_SEQUENCING: &str = "https://sbol.io/ns/capability#SangerSequencing";
pub(crate) const DNA_QUANTIFICATION: &str = "https://sbol.io/ns/capability#DnaQuantification";
pub(crate) const COLD_STORAGE: &str = "https://sbol.io/ns/capability#ColdStorage";
pub(crate) const WASTE_HANDLING: &str = "https://sbol.io/ns/capability#WasteHandling";
