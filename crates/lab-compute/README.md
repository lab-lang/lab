# `lab-compute`

`lab-compute` is the control-plane boundary for finite, artifact-producing
compute jobs. It normalizes hardware catalogs, job identity, lifecycle state,
logs, cancellation, and artifact retrieval without knowing what a robot task
means or how an Isaac environment is constructed.

C3 is the first and primary provider. The adapter invokes C3's installed CLI
and consumes only `--json` responses for automation. Authentication remains
outside repository state: callers may pass `C3_API_KEY` to the child process,
and the `lab` CLI can read it from an ignored `.env` file without evaluating
that file as shell code.

The provider trait accepts a provider-ready project directory at submission.
Turning a robot task, embodiment binding, and training configuration into that
directory belongs to the robot-learning integration. This keeps C3 placement
and artifact transport out of both the scientific task and the trainer.

Tests use a temporary fake `c3` executable. No crate test can submit remote
compute or require a C3 account.
