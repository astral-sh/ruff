## What it does

Checks whether the name passed to `ParamSpec` differs from the assigned variable, or if ty cannot
determine the value of a string passed to the `name` argument.

## Why is this bad?

The name argument identifies the parameter specification in a callable's signature. A mismatch can
make it harder to connect that signature to the definition.

## Examples

```python
from typing import ParamSpec

# error: "Expected "P", got "Q""
P = ParamSpec("Q")


def f(x: str):
    # error: "Cannot verify that a value of type `str` matches "Q""
    Q = ParamSpec(x)
```

## Rule history

- v0.0.107: Split out from `mismatched-type-name`.
