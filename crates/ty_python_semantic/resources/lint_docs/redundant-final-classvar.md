## What it does

Checks for redundant combinations of the `ClassVar` and `Final` type qualifiers.

## Why is this bad?

An attribute that is marked `Final` in a class body is implicitly a class variable.
Marking it as `ClassVar` is therefore redundant.

Note that this diagnostic is not emitted for dataclass fields, where
`ClassVar[Final[int]]` has a distinct meaning from `Final[int]`.

## Examples

```python
from typing import ClassVar, Final


class C:
    # redundant
    x: ClassVar[Final[int]] = 1  # error
    # redundant
    y: Final[ClassVar[int]] = 1  # error
```
