#!/bin/sh
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <generated-bundle-directory> [opentrons-python]" >&2
  exit 2
fi

bundle=$1
python=${2:-.lab/opentrons-venv/bin/python}
if [ ! -x "$python" ]; then
  echo "Opentrons Python interpreter is not executable: $python" >&2
  exit 2
fi

protocol_list=$(mktemp "${TMPDIR:-/tmp}/lab-opentrons-flex-protocols.XXXXXX")
trap 'rm -f "$protocol_list"' EXIT HUP INT TERM
find "$bundle" -type f -name '*_protocol.json' -print | sort > "$protocol_list"
if [ ! -s "$protocol_list" ]; then
  echo "No generated *_protocol.json files found under $bundle" >&2
  exit 2
fi

config_dir="$bundle/.opentrons-config"
mkdir -p "$config_dir"
while IFS= read -r protocol; do
  echo "Analyzing $protocol"
  OT_API_CONFIG_DIR="$config_dir" "$python" -m opentrons.cli analyze --check "$protocol"
done < "$protocol_list"
