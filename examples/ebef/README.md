# EBEF reference facility

This package exercises Lab's SBOLInventory ingestion against a public-data model of Caltech's [Resnick Ecology and Biosphere Engineering Facility](https://resnick.caltech.edu/resource-centers/ecology-and-biosphere-engineering-facility-ebef).

## What EBEF is

The EBEF is a shared wet-lab resource in Caltech's Resnick Sustainability Center for studying life across spatial scales. Caltech describes support for isolating, cultivating, and genetically manipulating diverse microorganisms; plant cultivation; fluorescence in situ hybridization and microscopy; protein expression and purification; and anaerobic techniques. The facility has dedicated microbiology and microscopy space in the basement and plant-cultivation space on the first floor. Its laboratory spaces are designed for BSL2+ containment.

The public equipment page is the factual source for this example. It describes the kinds of spaces and equipment present, but it is not an asset registry or control contract. The checked-in graph records the page as `prov:wasDerivedFrom` and deliberately omits facts that would require access to EBEF's internal records.

## What the reference catalog contains

The catalog contains one `fac:Facility`, 12 `fac:Zone` objects, 28 `fac:Asset` objects, and 30 owned `fac:CapabilityOffering` objects. The following tree shows the modeled containment and composition. A chamber or growth cabinet is located in a room as an Asset and establishes a separate controlled Zone; instruments inside that environment are located in the established Zone.

```text
Resnick Ecology and Biosphere Engineering Facility [Facility]
├── Main lab, basement [Room]
│   ├── Microbiology lab [WorkArea]
│   │   ├── Anaerobic chamber 1 [EnvironmentController]
│   │   │   └── Chamber 1 interior [ContainmentZone]
│   │   │       ├── Hamilton Microlab Prep
│   │   │       └── 96-well potentiostat
│   │   ├── Anaerobic chamber 2 [EnvironmentController]
│   │   │   └── Chamber 2 interior [ContainmentZone]
│   │   │       └── Swinging-bucket centrifuge
│   │   ├── Three Eppendorf S44i shaking incubators
│   │   ├── Grouped static incubators
│   │   ├── Agilent BioTek Epoch 2 plate reader
│   │   ├── ProFlex thermocycler
│   │   │   ├── Independently runnable block 1 [FunctionalUnit]
│   │   │   ├── Independently runnable block 2 [FunctionalUnit]
│   │   │   └── Independently runnable block 3 [FunctionalUnit]
│   │   ├── Azure 300 gel imager
│   │   ├── DNA and protein electrophoresis station
│   │   ├── Six-foot biosafety cabinet
│   │   └── AMSCO 630LS autoclave
│   ├── Microscopy lab [WorkArea]
│   │   ├── Dragonfly spinning-disk confocal microscope
│   │   └── Plasma cleaner
│   ├── Media preparation room [WorkArea]
│   │   └── Media and buffer preparation station
│   └── Freezer room [StorageZone]
│       └── Grouped 4 C, -20 C, and -70 C storage
└── Plant lab, first floor [Room]
    ├── Two Conviron Gen1000 chambers, one Gen2000, and one GR48
    │   └── Each chamber establishes its own [EnvironmentZone]
    ├── Four-foot biosafety cabinet
    └── Soil and plant-waste autoclave
```

The corresponding capability surface is summarized below. Names such as `cap:LiquidHandling` abbreviate IRIs in the `https://sbol.io/ns/capability#` namespace.

| Modeled area | Assets represented | Capability offerings represented |
| --- | --- | --- |
| Basement microbiology | Anaerobic chambers and their internal instruments, three shaking incubators, grouped static incubators, Epoch 2, ProFlex and three child blocks, gel imaging and electrophoresis equipment, biosafety cabinet, and autoclave | `AnaerobicEnvironmentControl`, `LiquidHandling`, `ElectrochemicalMeasurement`, `Centrifugation`, `ShakingIncubation`, `StaticIncubation`, `AbsorbanceMeasurement`, `Incubation`, `ThermalCycling`, `GelImaging`, `Electrophoresis`, `BiosafetyContainment`, `SteamSterilization` |
| Basement microscopy | Dragonfly confocal microscope and plasma cleaner | `ConfocalMicroscopy`, `PlasmaCleaning` |
| Media preparation | One workstation representing the publicly described balances, pH meter, MilliQ water, and preparation hood | `MediaPreparation`, `PhMeasurement`, `WaterPurification` |
| Freezer room | One placeholder storage Asset because the public page does not identify each reservable unit | `ColdStorage` with documented temperatures |
| First-floor plant lab | Four programmable growth chambers, biosafety cabinet, and autoclave | `PlantGrowth`, `BiosafetyContainment`, `SteamSterilization` |

The public page also lists miscellaneous shared equipment such as pipettes, vortexers, water baths, centrifuges, a stereomicroscope, and an ice machine. The reference graph does not invent independently reservable Asset identities for those items when the public source does not provide them.

## How the physical infrastructure becomes SBOLInventory

| Public or operational fact | SBOLInventory representation in `inventory/ebef.ttl` |
| --- | --- |
| The EBEF is one governed laboratory resource | One `fac:Facility` with identity `https://example.org/ebef/facility` |
| Basement and first-floor laboratory areas contain more specific work areas | `fac:Zone` objects connected with `fac:parentZone` |
| An instrument or controlled chamber is installed in a place | A `fac:Asset` with `fac:locatedIn` pointing to the containing Zone |
| An anaerobic chamber or plant-growth cabinet creates a controlled interior | The controller Asset points to a distinct Zone with `fac:establishesZone` |
| The Microlab Prep and potentiostat are physically inside chamber 1 | Their `fac:locatedIn` values point to the chamber 1 interior, not merely the microbiology room |
| The ProFlex has three independently runnable blocks | One parent Asset plus three `fac:FunctionalUnit` child Assets connected with `fac:partOf`; thermal-cycling offerings belong to the child blocks |
| An installed Asset can perform an operation | The Asset owns a `fac:CapabilityOffering` whose `fac:capabilityKind` is a stable `cap:` IRI |
| The public source gives a capacity, temperature, atmosphere, or feature | The offering or Zone owns typed `fac:PropertyValue` objects, using QUDT unit IRIs where applicable |
| The source describes several units but does not identify each one | A clearly labeled grouped placeholder Asset is used instead of inventing serializable unit identities |

For example, this shortened Turtle fragment captures the distinction between the chamber, the environment it establishes, and the instrument inside that environment:

```turtle
@prefix cap: <https://sbol.io/ns/capability#> .
@prefix ex: <https://example.org/ebef/> .
@prefix fac: <https://sbol.io/ns/facility#> .
@prefix sbol: <http://sbols.org/v3#> .

ex:anaerobic_chamber_1
    a sbol:TopLevel, fac:Asset ;
    fac:locatedIn ex:microbiology_lab ;
    fac:establishesZone ex:anaerobic_chamber_1_interior .

ex:anaerobic_chamber_1_interior
    a sbol:TopLevel, fac:Zone ;
    fac:parentZone ex:microbiology_lab ;
    fac:zoneKind fac:ContainmentZone .

ex:microlab_prep
    a sbol:TopLevel, fac:Asset ;
    fac:locatedIn ex:anaerobic_chamber_1_interior ;
    fac:capability <https://example.org/ebef/microlab_prep/liquid_handling> .

<https://example.org/ebef/microlab_prep/liquid_handling>
    a sbol:Identified, fac:CapabilityOffering ;
    fac:capabilityKind cap:LiquidHandling ;
    fac:qualification fac:Described ;
    fac:controlMode fac:UnspecifiedControl .
```

## How Lab loads the inventory

There is no separate `inventory.toml` model. [`lab.toml`](lab.toml) contains only the package configuration and a package-relative pointer to the actual SBOLInventory RDF document:

```toml
[inventory]
document = "inventory/ebef.ttl"
```

Lab infers Turtle from the `.ttl` extension, validates the document as both SBOL 3 and SBOLInventory Profile 0.2, and selects its Facility. The `facility` selector is omitted because [`inventory/ebef.ttl`](inventory/ebef.ttl) contains exactly one Facility. An equivalent explicit selection would be:

```toml
[inventory]
document = "inventory/ebef.ttl"
facility = "https://example.org/ebef/facility"
```

The package intentionally has no `[[execution.adapters]]` entries. A public equipment description does not establish an installed control path, adapter configuration, credentials, or permission to operate hardware. Consequently, every public capability offering is `fac:Described` with `fac:UnspecifiedControl`; none is silently promoted to plannable or executable.

Run the validation path from the repository root with:

```bash
lab check examples/ebef
```

## Provenance and limits

The catalog was generated by `sbol-inventory`'s `ebef_catalog` example at sbol-rs revision `2ecae3718ebb87dbdbf7112ed4d7f42c0155eea4`, using SBOLInventory Profile 0.2 artifacts from revision `7d8cb750dd2d5e3c6c7602e575c3a551b890724f`. It contains one facility, 12 zones, 28 assets, and 30 capability offerings.

This is an architectural example, not an operational source of truth. It omits serial numbers, exact room and deck positions, network details, booking state, access-control policy, calibration and maintenance records, material lots, runtime adapters, and execution claims. The graph uses `fac:isActive true` to model catalog availability in this illustrative snapshot, but the public web page is not a live availability system; the value must not be treated as evidence that an instrument is presently bookable, calibrated, or safe to use.

The graph is generated from the typed Rust authoring example rather than maintained by hand. Its source page and access date are preserved in the Facility description and provenance, while the exact generated-file SHA-256 is pinned by the `lab-inventory` EBEF test.

## From description to simulation

The [acceptance scenario](acceptance/README.md) extends this graph in isolation with explicitly synthetic simulation Assets. It exercises a multi-Asset plate growth and absorbance plan without changing the qualification or control-mode claims on EBEF's physical equipment.
