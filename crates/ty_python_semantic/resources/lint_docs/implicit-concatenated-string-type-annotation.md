## What it does

Checks for implicit concatenated strings in type annotation positions.

## Why is this bad?

Static analysis tools like ty can't analyze type annotations that use implicit concatenated strings.

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
