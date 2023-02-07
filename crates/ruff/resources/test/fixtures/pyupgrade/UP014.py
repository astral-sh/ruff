from typing import NamedTuple
import typing

# with complex annotations
NT1 = NamedTuple("NT1", [("a", int), ("b", tuple[str, ...])])

# with default values as list
NT2 = NamedTuple(
    "NT2",
    [("a", int), ("b", str), ("c", list[bool])],
    defaults=["foo", [True]],
)

# with namespace
NT3 = typing.NamedTuple("NT3", [("a", int), ("b", str)])

# with too many default values
NT4 = NamedTuple(
    "NT4",
    [("a", int), ("b", str)],
    defaults=[1, "bar", "baz"],
)
