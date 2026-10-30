## What it does

Checks for dataclass fields annotated with bare `Final` whose types cannot be inferred from a
default value, default factory, or converter.

## Why is this bad?

A bare `Final` annotation relies on an initial value to determine the field's type. Without an
explicit type argument or a source for inference, the field's type is unknown.

Add a type argument to `Final` or provide a default value, factory, or converter.

## Examples

```python
from dataclasses import dataclass, field
from typing import Final


@dataclass
class Example:
    value: Final  # error: [untyped-final-field]
    other: Final = field()  # error: [untyped-final-field]
```

Use an explicit type argument:

```python
from dataclasses import dataclass, field
from typing import Final


@dataclass
class Example:
    value: Final[int]
    other: Final[int] = field()
```

Or provide a default from which to infer the type:

```python
from dataclasses import dataclass
from typing import Final


class Value: ...


@dataclass
class Example:
    value: Final = Value()
```
