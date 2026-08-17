project: lab-isaac-capability
job_name: isaac-lab-capability
script: c3/probe.sh
hardware: l40
time: "00:20:00"

capacity:
  on_unavailable: fail
  max_wait_minutes: 10

python:
  project: ./c3/runtime
