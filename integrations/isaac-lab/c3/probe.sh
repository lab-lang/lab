#!/usr/bin/env bash
set -euo pipefail

export ACCEPT_EULA=Y
export PRIVACY_CONSENT=Y

python c3/probe.py \
  --task tests/fixtures/robot-tasks/task.json \
  --binding examples/golden-gate-plate-transfer.binding.toml \
  --num-envs 32 \
  --steps 8
