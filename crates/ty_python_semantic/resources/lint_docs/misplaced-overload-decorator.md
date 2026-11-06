## What it does

Checks for `@final` or `@override` decorators applied to the wrong definition of an overloaded
method. Put the decorator on the implementation when there is one, or on the first overload when
there is no implementation.

## Why is this bad?

These decorators apply to the whole method. Placing one on an individual overload suggests that only
that overload is final or overrides a superclass method.

## Example

```pyi
from typing_extensions import overload, override

class Decoder:
    def decode(self, data: bytes) -> str: ...

class BatchDecoder(Decoder):
    @overload
    def decode(self, data: bytes) -> str: ...
    @overload
    @override
    def decode(self, data: list[bytes]) -> list[str]: ...  # error
```

Move `@override` to the first overload of `BatchDecoder.decode`.

## References

- [Typing specification: Invalid overload definitions](https://typing.python.org/en/latest/spec/overload.html#invalid-overload-definitions)
