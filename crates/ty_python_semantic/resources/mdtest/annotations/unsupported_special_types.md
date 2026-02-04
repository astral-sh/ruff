# Unsupported special types

## Functional classes

We do not understand the functional syntax for creating `TypedDict`s or `Enum`s yet. But we also do
not emit false positives when these are used in type expressions.

```py
import collections
import enum
import typing

MyEnum = enum.Enum("MyEnum", ["foo", "bar", "baz"])
MyIntEnum = enum.IntEnum("MyIntEnum", ["foo", "bar", "baz"])
MyTypedDict = typing.TypedDict("MyTypedDict", {"foo": int})

def f(a: MyEnum, b: MyTypedDict): ...
```

## No false positives for subscripting a class generic over a `TypeVarTuple`

We don't support `TypeVarTuple` yet, but we also try to avoid emitting false positives when you
subscript classes generic over a `TypeVarTuple`:

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Generic, TypeVarTuple, Unpack

Ts = TypeVarTuple("Ts")

class Foo(Generic[Unpack[Ts]]): ...

x: Foo[int, str, bytes]  # fine

class Bar(Generic[*Ts]): ...

y: Bar[int, str, bytes]  # fine

class Baz[*Ts]: ...

z: Baz[int, str, bytes]  # fine
```

And we also provide some basic validation in some cases:

```py
# error: [invalid-generic-class] "`TypeVarTuple` must be unpacked with `*` or `Unpack[]` when used as an argument to `Generic`"
class Spam(Generic[Ts]): ...
```
