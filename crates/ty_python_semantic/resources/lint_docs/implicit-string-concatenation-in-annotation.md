## What it does

Checks for [type expressions][type expression] written as two or more adjacent string literals.

## Why is this bad?

ty can't resolve a string annotation when a [type expression] spans multiple string literals.

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

Note that this rule is only applied to [type expressions][type expression] in annotations. Regions
in type annotations that are interpreted as _value_ expressions -- for example, the second argument
to `Annotated[]` -- are not reported by this rule:

<!-- fmt:off -->

```python
from typing import Annotated

# the first argument to `Annotated[]` is interpreted as a type expression, so is reported
count: Annotated["in" "t", "item count"]  # error

# all remaining arguments to `Annotated[]` are interpreted as value expressions, so are not reported
total: Annotated[int, "item " "count"]  # ok
```

<!-- fmt:on -->

## Rule history

- v0.0.102: Renamed from `implicit-concatenated-string-type-annotation`.

    `implicit-concatenated-string-type-annotation` remains as a compatibility alias for this rule.

[type expression]: https://typing.python.org/en/latest/spec/annotations.html#type-and-annotation-expressions
