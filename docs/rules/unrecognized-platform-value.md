# unrecognized-platform-value (PYI008)

Derived from the **flake8-pyi** linter.

### What it does
xxxx

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