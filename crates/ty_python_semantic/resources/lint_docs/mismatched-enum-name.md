## What it does

Checks that the name passed to a functional enum constructor, such as `Enum`, `IntEnum`, or `Flag`,
matches the variable it is assigned to. This rule also warns if ty cannot determine the value of the
name argument.

## Why is this bad?

The name argument becomes the enum class's `__name__`. A member's representation can therefore refer
to a different class name than the one used in the source.

## Examples

```python
from enum import Enum

Color = Enum("Colour", "RED BLUE")  # error: [mismatched-enum-name]
```
