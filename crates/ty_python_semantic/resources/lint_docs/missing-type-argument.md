## What it does

Checks for generic types used without type parameters in type expressions.

## Why is this bad?

Using a generic type without specifying its type parameters results in the
type parameters being implicitly filled with `Unknown`, reducing the
precision of type checking. Explicit type parameters make the intended types
clear and enable the type checker to catch more errors.

## Examples

```python
import re


def handle(m: re.Match) -> str:  # error: [missing-type-argument]
    return m.string


# Use explicit type parameters instead:
def handle(m: re.Match[str]) -> str:
    return m.string
```
