# prefix-type-params (PYI001)

Derived from the **flake8-pyi** linter.

## What it does
Checks that type `TypeVar`, `ParamSpec`, and `TypeVarTuple` definitions in
stubs are prefixed with `_`.

## Why is this bad?
By prefixing type parameters with `_`, we can avoid accidentally exposing
names internal to the stub.

## Example
```python
from typing import TypeVar

T = TypeVar("T")
```

Use instead:
```python
from typing import TypeVar

_T = TypeVar("_T")
```