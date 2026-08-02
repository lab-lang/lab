# SDKs

SDKs expose Lab compilation in host-language-native APIs while sharing one compiler implementation.

- `rust/` is the primary ergonomic API over Lab Lang, the compiler, plans, and biological specifications.
- `python/` is a thin PyO3 binding over `rust/`; it returns the printed target-selected LAIR module (Design plus Protocol dialects) and a Python-native executable plan.

An SDK is not a separate compiler and must not introduce semantics that bypass specification, LAIR, or plan verification.
