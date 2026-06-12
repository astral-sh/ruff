## What it does

Checks for enum members that have explicit type annotations.

## Why is this bad?

The [typing spec] states that type checkers should infer a literal type
for all enum members. An explicit type annotation on an enum member is
misleading because the annotated type will be incorrect — the actual
runtime type is the enum class itself, not the annotated type.

In CPython's `enum` module, annotated assignments with values are still
treated as members at runtime, but the annotation will confuse readers of the code.

## Examples

```python
from enum import Enum


class Pet(Enum):
    CAT = 1  # OK
    # enum members should not be annotated
    DOG: int = 2  # error
```

Use instead:

```python
from enum import Enum


class Pet(Enum):
    CAT = 1
    DOG = 2
```

## References

- [Typing spec: Enum members](https://typing.python.org/en/latest/spec/enums.html#enum-members)

[typing spec]: https://typing.python.org/en/latest/spec/enums.html#enum-members
