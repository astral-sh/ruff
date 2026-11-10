## What it does

Checks for implicit string concatenation in type annotations.

## Why is this bad?

Static analysis tools like ty can't analyze type annotations that use implicit string concatenation.

## Examples

<!-- fmt:off -->

```python
from typing import Literal

def test() -> "Literal[" "5" "]":  # error
    return 5
```

<!-- fmt:on -->

Use instead:

```python
from typing import Literal


def test() -> "Literal[5]":
    return 5
```

## Rule history

- v0.0.101: Renamed from `implicit-concatenated-string-type-annotation`.

    `implicit-concatenated-string-type-annotation` remains as a compatibility alias for this rule.
