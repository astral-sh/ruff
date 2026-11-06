## What it does

Checks for overloaded functions and methods without a required implementation. An implementation can
be omitted in any of the following cases:

- The overloads are defined in a stub file (`.pyi`).
- All overloads are defined inside `if TYPE_CHECKING` blocks.
- The overloaded method is defined in a protocol.
- The method is defined in an abstract base class and every overload is decorated with
    `@abstractmethod`.

## Why is this bad?

The `@overload` definitions describe the calls a type checker should accept. A separate
implementation handles those calls at runtime. Calling an overloaded function without an
implementation raises `NotImplementedError`.

## Example

```py
from typing import overload


@overload
def foo() -> None: ...  # error
@overload
def foo(x: int) -> int: ...
```

## References

- [Python documentation: `@overload`](https://docs.python.org/3/library/typing.html#typing.overload)
