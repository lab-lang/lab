#!/bin/sh
set -eu

repository=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
project="$repository/crates/lab-compiler/src/backend/opentrons_ot2/python"

if [ "$#" -gt 1 ]; then
  echo "usage: $0 [generated-bundle-directory]" >&2
  exit 2
fi

cd "$project"
uv sync --locked --all-groups
uv run --locked ruff check .
uv run --locked ruff format --check .
uv run --locked mypy
uv run --locked pytest

if [ "$#" -eq 1 ]; then
  bundle=$1
  case "$bundle" in
    /*) ;;
    *) bundle="$repository/$bundle" ;;
  esac
  if [ ! -d "$bundle" ]; then
    echo "Generated bundle directory does not exist: $bundle" >&2
    exit 2
  fi

  protocol_list=$(mktemp "${TMPDIR:-/tmp}/lab-opentrons-python.XXXXXX")
  trap 'rm -f "$protocol_list"' EXIT HUP INT TERM
  find "$bundle" -type f -name '*_protocol.py' -print | sort > "$protocol_list"
  if [ ! -s "$protocol_list" ]; then
    echo "No generated *_protocol.py files found under $bundle" >&2
    exit 2
  fi

  while IFS= read -r protocol; do
    echo "Checking generated protocol $protocol"
    uv run --locked ruff check "$protocol"
    uv run --locked mypy --strict "$protocol"
    uv run --locked python -m py_compile "$protocol"
  done < "$protocol_list"
fi
