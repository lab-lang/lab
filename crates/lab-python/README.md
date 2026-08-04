# Lab Python SDK

The Python package is a thin PyO3 binding over `lab-sdk`; it does not reimplement parsing or compilation. The initial API compiles Lab Lang for the reference laboratory profile and returns both the printed target-selected LAIR module (Design plus Protocol dialects) and a Python-native executable plan.

```python
from lab import compile_lab_lang

compilation = compile_lab_lang(source)
print(compilation.ir)
print(compilation.plan["steps"])
```

Build or install it with Maturin from this directory. Once installed, run the Python boundary test with:

```sh
python -m unittest discover -s tests
```
