## What it does

Checks for inconsistent use of decorators on overloaded methods. If one definition uses
`@staticmethod` or `@classmethod`, every overload and the implementation, if present, must use the
same decorator.

## Why is this bad?

Python's runtime doesn't know about overloads, but it does have a different binding behavior for
instance methods, class methods and staticmethods. Overloads therefore need to be consistent about
their use of `@staticmethod` and `@classmethod` decorators.

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
