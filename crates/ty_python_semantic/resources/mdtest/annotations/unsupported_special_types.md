# Unsupported special types

We do not understand the functional syntax for creating `NamedTuple`s, `TypedDict`s or `Enum`s yet.
But we also do not emit false positives when these are used in type expressions.

```py
import collections
import enum
import typing

# TODO: should not error (requires understanding metaclass `__call__`)
MyEnum = enum.Enum("MyEnum", ["foo", "bar", "baz"])  # error: [too-many-positional-arguments]

MyTypedDict = typing.TypedDict("MyTypedDict", {"foo": int})
MyNamedTuple1 = typing.NamedTuple("MyNamedTuple1", [("foo", int)])
MyNamedTuple2 = collections.namedtuple("MyNamedTuple2", ["foo"])

def f(a: MyEnum, b: MyTypedDict, c: MyNamedTuple1, d: MyNamedTuple2): ...
```
