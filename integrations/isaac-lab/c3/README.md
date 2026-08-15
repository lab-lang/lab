# C3 Isaac Lab capability gate

This project is the first remote-compute gate for Lab robot learning. It asks
C3 for one L40-class GPU, installs a locked Isaac Lab runtime, launches the
existing plate-transfer environment headlessly, and returns one
`capability.json` artifact. It does not train a policy.

The tracked [`.c3`](../.c3) file deliberately requests `hardware: l40`.
Isaac Sim requires an RTX-capable GPU; C3's A100 and H100 classes are therefore
not valid substitutes even though they provide more CUDA memory.

Before any paid submission, validate the local credential and current catalog:

```sh
lab compute doctor
```

The actual capability run is an external, billable action and must be reviewed
before invoking `c3 deploy` from `integrations/isaac-lab`. Its runtime is
bounded at twenty minutes and fails rather than waiting on unavailable
capacity. C3 bills actual compute time, not the declared maximum.

The probe records only runtime and hardware facts needed to qualify the
environment. It never records environment variables wholesale, the C3 API
key, user identity, or other credentials.
