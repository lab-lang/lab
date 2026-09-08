# Contributing Hamilton STAR liquid classes

Liquid classes are versioned data in the STAR adapter profile. Adding a calibrated class does not require a Rust dispatch arm. A profile registers one or more complete libraries and selects one exact library ID and version:

```toml
[liquid_classes.selected_library]
id = "org.example.hamilton.liquid-classes"
version = "2.0.0"

[[liquid_classes.libraries]]
schema_version = "lab.hamilton-star-liquid-classes.v1"
id = "org.example.hamilton.liquid-classes"
version = "2.0.0"

[[liquid_classes.libraries.classes]]
id = "org.example.hamilton.aqueous-small-tip"
version = "1.3.0"
priority = 10

[liquid_classes.libraries.classes.applicability]
liquids = ["aqueous"]
techniques = ["distribution", "mix"]
tips = ["tip_rack_50ul_filter"]
source_labware = ["sample_tubes_24"]
destination_labware = ["pcr_plate_96"]
min_volume_ul = 0.0
max_volume_ul = 50.0

[liquid_classes.libraries.classes.correction]
points = [
  { target_ul = 0.0, commanded_ul = 0.0 },
  { target_ul = 20.0, commanded_ul = 23.0 },
  { target_ul = 50.0, commanded_ul = 55.0 },
]

[liquid_classes.libraries.classes.speeds]
aspirate_ul_s = 80.0
dispense_ul_s = 100.0
aspirate_mix_ul_s = 70.0
dispense_mix_ul_s = 90.0

[liquid_classes.libraries.classes.lld]
mode = "profile"
gamma_sensitivity = 1
pressure_sensitivity = 1

[liquid_classes.libraries.classes.margins]
aspiration_immersion_mm = 2.0
bottom_standoff_mm = 0.5
dispense_clearance_mm = 2.0
lld_search_clearance_mm = 5.0

[liquid_classes.libraries.classes.calibration]
source = "Example Lab gravimetric calibration campaign"
source_version = "campaign-2026-09"
instrument = "STARlet serial 1234"
performed_by = "Example Lab automation team"
observed_at = "2026-09-01"
notes = "Qualified with the named tip and labware combination."
```

Every registered library is schema and semantics validated, including libraries that are not selected. Library selection is an exact ID and version lookup. Within the selected library, classes are matched from liquid, technique, tip type, source labware, destination labware, and volume. Priority and applicability specificity resolve deliberate overlaps; an equally ranked overlap between distinct class IDs is rejected.

The compiler normalizes each library and class before calculating its SHA-256 digest. Authors provide IDs, versions, applicability, behavior, and provenance, but never author the digest. The reviewed invocation manifest and manual pin the selected library ID, version, and digest. The manifest and STAR run also pin every selected class ID, version, digest, and its operational evidence.

The built-in example is [`liquid_classes.v1.toml`](liquid_classes.v1.toml). Copy its complete field structure when preparing a library or translating an export through `import_venus_record_json`, then validate the ordinary STAR adapter profile through the application adapter registry.
