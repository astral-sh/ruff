## What it does

Checks for the creation of invalid `ParamSpec`s

## Why is this bad?

There are several requirements that you must follow when creating a `ParamSpec`.

## Examples

```python
from typing import ParamSpec

P1 = ParamSpec("P1")  # okay
P2 = ParamSpec()  # error: ParamSpec requires a name
```

## References

- [Typing spec: ParamSpec](https://typing.python.org/en/latest/spec/generics.html#paramspec)
