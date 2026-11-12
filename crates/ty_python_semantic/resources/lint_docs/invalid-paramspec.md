## What it does

Checks for the creation of invalid `ParamSpec`s

## Why is this bad?

There are several requirements that you must follow when creating a `ParamSpec`.

## Examples

```python
from typing import ParamSpec

P1 = ParamSpec("P1")  # okay
# ParamSpec requires a name
P2 = ParamSpec()  # error
```

## References

- [Typing spec: ParamSpec](https://typing.python.org/en/latest/spec/generics.html#paramspec)

## Rule history

- v0.0.102: Checks on the use of `P.args` and `P.kwargs` moved to `invalid-paramspec-components`.
    Use the new name in rule settings and suppression comments for these checks.
