#!/bin/sh
set -eu

repository=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
project="$repository/crates/lab-python"
golden_gate_example="$repository/examples/golden-gate-python/golden_gate"
extended_example="$repository/examples/golden-gate-extended-python/golden_gate_extended"

cd "$project"
uv sync --locked --all-groups
uv run --locked ruff check . "$golden_gate_example" "$extended_example"
uv run --locked ruff format --check . "$golden_gate_example" "$extended_example"
uv run --locked mypy
uv run --locked mypy "$golden_gate_example" "$extended_example"
uv run --locked pytest
