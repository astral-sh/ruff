## What it does

Checks for class-scoped type variables in an explicit annotation of the `self` parameter of
`__init__`.

## Why is this bad?

An explicit `self` annotation on `__init__` can determine the type arguments of the constructed
class. Referring to the class's own type variables in this annotation can make their meaning
ambiguous. The
[typing specification](https://typing.python.org/en/latest/spec/constructors.html#init-method)
requires function-scoped type variables instead.

## Example

```toml
[environment]
python-version = "3.12"
```

```python
class Container[T]:
    # error: [invalid-init-type-variable]
    def __init__(self: "Container[list[T]]", value: T) -> None: ...
```

Use a function-scoped type variable instead:

```python
class ListContainer[T]:
    def __init__[U](self: "ListContainer[list[U]]", value: U) -> None: ...
```

If the receiver annotation does not change the class's type arguments, it can be omitted:

```python
class Box[T]:
    def __init__(self, value: T) -> None: ...
```

This restriction also applies to type variables declared with legacy syntax:

```python
from typing import Generic, TypeVar

T = TypeVar("T")


class LegacyContainer(Generic[T]):
    # error: [invalid-init-type-variable]
    def __init__(self: "LegacyContainer[list[T]]", value: T) -> None: ...
```

## References

- [Typing specification: `__init__` method](https://typing.python.org/en/latest/spec/constructors.html#init-method)
