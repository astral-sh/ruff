## What it does

Checks for enum classes that are also generic.

## Why is this bad?

Enum classes cannot be generic. Python does not support generic enums:
attempting to create one will either result in an immediate `TypeError`
at runtime, or will create a class that cannot be specialized in the way
that a normal generic class can.

## Examples

```toml
[environment]
python-version = "3.12"
```

```python
from enum import Enum
from typing import Generic, TypeVar

T = TypeVar("T")


# enum class cannot be generic (class creation fails with `TypeError`)
class E[T](Enum):  # error
    A = 1


# enum class cannot be generic (class creation fails with `TypeError`)
class F(Enum, Generic[T]):  # error
    A = 1


# enum class cannot be generic -- the class creation does not immediately fail...
class G(Generic[T], Enum):  # error
    A = 1


# ...but this raises `KeyError`:
x: G[int]
```

## References

- [Python documentation: Enum](https://docs.python.org/3/library/enum.html)
