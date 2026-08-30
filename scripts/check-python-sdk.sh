#!/bin/sh
set -eu

repository=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
project="$repository/crates/lab-python"

cd "$project"
uv sync --locked --all-groups
uv run --locked ruff check .
uv run --locked ruff format --check .
uv run --locked mypy
uv run --locked pytest
uv run --locked python -m unittest discover -s "$repository/scripts/tests" -p 'test_*.py'
