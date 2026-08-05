# Lab Python SDK

The Python package is a thin PyO3 binding over `lab-sdk`; it does not reimplement parsing or semantic checking. Its API returns the backend-neutral checked module as Python-native data.

```python
from lab import compile_lab_module

module = compile_lab_module(source)
print(module["declarations"])
```

Build or install it with Maturin from this directory. Once installed, run the Python boundary test with:

```sh
python -m unittest discover -s tests
```
