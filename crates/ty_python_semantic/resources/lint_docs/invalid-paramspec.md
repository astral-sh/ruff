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
