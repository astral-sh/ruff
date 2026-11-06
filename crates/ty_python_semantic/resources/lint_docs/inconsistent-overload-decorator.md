## What it does

Checks for inconsistent use of decorators on overloaded methods. If one definition uses
`@staticmethod` or `@classmethod`, every overload and the implementation, if present, must use the
same decorator.

## Why is this bad?

Python binds a method the same way for every overload. If one overload describes an instance method
and another describes a static method, they disagree about whether a call through an instance
implicitly passes that instance as the first argument.

## Example

```pyi
from typing import overload

class Reader:
    @staticmethod
    @overload
    def read() -> str: ...
    @overload
    def read(self, size: int) -> str: ...  # error
```

If `read` is an instance method, remove `@staticmethod` and add `self` to the first overload. Both
overloads then account for the implicit `self` in `reader.read()` and `reader.read(10)`.

## References

- [Typing specification: Invalid overload definitions](https://typing.python.org/en/latest/spec/overload.html#invalid-overload-definitions)
