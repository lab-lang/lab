"""Typed discovery and validation for the adapters built into Lab.

The Rust compiler owns adapter identities, semantic capability support, control modes, run formats,
service claims, and operational profile schemas. Python reads that same catalog and asks the same
validator to canonicalize profiles; it does not maintain a parallel device registry.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, cast

from ._native import lab_adapter_catalog as _lab_adapter_catalog
from ._native import validate_lab_adapter_profile as _validate_lab_adapter_profile


@dataclass(frozen=True, slots=True)
class AdapterServices:
    planning: bool
    lowering: bool
    simulation: bool
    runtime: bool


@dataclass(frozen=True, slots=True)
class ValidatedAdapterProfile:
    """Canonical non-secret operational configuration for one adapter binding."""

    format: str
    schema_version: str
    compiler_version: str
    name: str
    driver: str
    canonical_toml: str
    canonical_json: dict[str, Any]
    sha256: str


@dataclass(frozen=True, slots=True)
class AdapterDescriptor:
    """One implementation contract, separate from every facility Asset."""

    id: str
    display_name: str
    manufacturer: str | None
    capabilities: tuple[str, ...]
    features: tuple[str, ...]
    control_modes: tuple[str, ...]
    accepted_run_formats: tuple[str, ...]
    emitted_run_formats: tuple[str, ...]
    services: AdapterServices
    profile_schema: dict[str, Any]
    default_profile: ValidatedAdapterProfile


@dataclass(frozen=True, slots=True)
class AdapterCatalog:
    format: str
    compiler_version: str
    profile_schema_version: str
    adapters: tuple[AdapterDescriptor, ...]

    def get(self, driver: str) -> AdapterDescriptor:
        """Return one exact driver or raise ``KeyError``; manufacturer names are never selectors."""

        for adapter in self.adapters:
            if adapter.id == driver:
                return adapter
        raise KeyError(driver)


def _profile(raw: dict[str, Any]) -> ValidatedAdapterProfile:
    return ValidatedAdapterProfile(
        format=cast(str, raw["format"]),
        schema_version=cast(str, raw["schema_version"]),
        compiler_version=cast(str, raw["compiler_version"]),
        name=cast(str, raw["name"]),
        driver=cast(str, raw["driver"]),
        canonical_toml=cast(str, raw["canonical_toml"]),
        canonical_json=cast(dict[str, Any], raw["canonical_json"]),
        sha256=cast(str, raw["sha256"]),
    )


def catalog() -> AdapterCatalog:
    """Load the authoritative adapter catalog from the installed Lab compiler."""

    raw = cast(dict[str, Any], json.loads(_lab_adapter_catalog()))
    descriptors = []
    for item in cast(list[dict[str, Any]], raw["adapters"]):
        services = cast(dict[str, Any], item["services"])
        descriptors.append(
            AdapterDescriptor(
                id=cast(str, item["id"]),
                display_name=cast(str, item["display_name"]),
                manufacturer=cast(str | None, item.get("manufacturer")),
                capabilities=tuple(cast(list[str], item["capabilities"])),
                features=tuple(cast(list[str], item["features"])),
                control_modes=tuple(cast(list[str], item["control_modes"])),
                accepted_run_formats=tuple(cast(list[str], item["accepted_run_formats"])),
                emitted_run_formats=tuple(cast(list[str], item["emitted_run_formats"])),
                services=AdapterServices(
                    planning=cast(bool, services["planning"]),
                    lowering=cast(bool, services["lowering"]),
                    simulation=cast(bool, services["simulation"]),
                    runtime=cast(bool, services["runtime"]),
                ),
                profile_schema=cast(dict[str, Any], item["profile_schema"]),
                default_profile=_profile(cast(dict[str, Any], item["default_profile"])),
            )
        )
    return AdapterCatalog(
        format=cast(str, raw["format"]),
        compiler_version=cast(str, raw["compiler_version"]),
        profile_schema_version=cast(str, raw["profile_schema_version"]),
        adapters=tuple(descriptors),
    )


def validate_profile(
    driver: str,
    contents: str,
    *,
    name: str | None = None,
) -> ValidatedAdapterProfile:
    """Validate profile TOML using the implementation selected by the exact driver ID."""

    return _profile(
        cast(
            dict[str, Any],
            json.loads(_validate_lab_adapter_profile(driver, name or driver, contents)),
        )
    )


def validate_profile_file(driver: str, path: str | Path) -> ValidatedAdapterProfile:
    """Read and validate one profile, using its file stem as review metadata."""

    profile_path = Path(path)
    return validate_profile(driver, profile_path.read_text(), name=profile_path.stem)


__all__ = [
    "AdapterCatalog",
    "AdapterDescriptor",
    "AdapterServices",
    "ValidatedAdapterProfile",
    "catalog",
    "validate_profile",
    "validate_profile_file",
]
