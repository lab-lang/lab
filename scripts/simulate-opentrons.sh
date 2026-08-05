#!/bin/sh
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <generated-bundle-directory> [opentrons_simulate]" >&2
  exit 2
fi

bundle=$1
simulator=${2:-.lab/opentrons-venv/bin/opentrons_simulate}
if [ ! -x "$simulator" ]; then
  echo "Opentrons simulator is not executable: $simulator" >&2
  exit 2
fi

protocol_list=$(mktemp "${TMPDIR:-/tmp}/lab-opentrons-protocols.XXXXXX")
trap 'rm -f "$protocol_list"' EXIT HUP INT TERM
find "$bundle" -type f -name '*_protocol.py' -print | sort > "$protocol_list"
if [ ! -s "$protocol_list" ]; then
  echo "No generated *_protocol.py files found under $bundle" >&2
  exit 2
fi

config_dir="$bundle/.opentrons-config"
mkdir -p "$config_dir"
while IFS= read -r protocol; do
  echo "Simulating $protocol"
  OT_API_CONFIG_DIR="$config_dir" "$simulator" -o nothing "$protocol"
done < "$protocol_list"
