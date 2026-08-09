# Vendored Opentrons labware definitions

These files are unmodified labware-schema-2 definitions published by Opentrons under the Apache-2.0 license, vendored from the `v8.4.0` tag of <https://github.com/Opentrons/opentrons> at `shared-data/labware/definitions/2/{loadName}/{version}.json`.

Each file is named `{loadName}_v{version}.json` and embeds verbatim into every emitted protocol document that loads the labware, which is what lets the builder validate well names and volumes at construction time.
