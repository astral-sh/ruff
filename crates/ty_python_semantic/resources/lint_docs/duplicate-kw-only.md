## What it does

Checks for dataclass definitions with more than one field
annotated with `KW_ONLY`.

## Why is this bad?

`dataclasses.KW_ONLY` is a special marker used to
emulate the `*` syntax in normal signatures.
It can only be used once per dataclass.

Attempting to annotate two different fields with
it will lead to a runtime error.

## Examples

```python
from dataclasses import dataclass, KW_ONLY


# Crash at runtime
@dataclass
class A:  # error
    b: int
    _1: KW_ONLY
    c: str
    _2: KW_ONLY
    d: bytes
```
