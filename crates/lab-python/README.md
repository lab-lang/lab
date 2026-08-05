# Lab Python SDK

The Python package is a thin PyO3 binding over `lab-compiler`; it does not reimplement parsing or semantic checking. Its API returns the backend-neutral checked module as Python-native data.

```python
from lab import compile_lab_module

module = compile_lab_module(source)
print(module["declarations"])
```

The Python module, native extension, tests, linter, and strict typechecker are one maintained unit. Run every gate from the repository root with:

```sh
scripts/check-python-sdk.sh
```
