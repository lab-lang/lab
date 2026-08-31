#!/bin/sh
set -eu

repository=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <generated-bundle-directory>" >&2
  exit 2
fi

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
  uvx ruff check "$protocol"
  uvx --from mypy mypy --strict --ignore-missing-imports "$protocol"
  python3 -m py_compile "$protocol"
done < "$protocol_list"
