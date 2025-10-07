# Unsupported special types

We do not understand the functional syntax for creating `NamedTuple`s or `Enum`s yet. But we also do
not emit false positives when these are used in type expressions.

```py
import collections
import enum
import typing

MyEnum = enum.Enum("MyEnum", ["foo", "bar", "baz"])
MyIntEnum = enum.IntEnum("MyIntEnum", ["foo", "bar", "baz"])
MyNamedTuple1 = typing.NamedTuple("MyNamedTuple1", [("foo", int)])
MyNamedTuple2 = collections.namedtuple("MyNamedTuple2", ["foo"])

def f(a: MyEnum, c: MyNamedTuple1, d: MyNamedTuple2): ...
```
