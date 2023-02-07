from typing import TypedDict, NotRequired, Literal
import typing

# dict literal
MyType1 = TypedDict("MyType1", {"a": int, "b": str})

# dict call
MyType2 = TypedDict("MyType2", dict(a=int, b=str))

# kwargs
MyType3 = TypedDict("MyType3", a=int, b=str)

# Empty TypedDict
MyType4 = TypedDict("MyType4")

# Literal values
MyType5 = TypedDict("MyType5", {"a": "hello"})
MyType6 = TypedDict("MyType6", a="hello")

# NotRequired
MyType7 = TypedDict("MyType7", {"a": NotRequired[dict]})

# total
MyType8 = TypedDict("MyType8", {"x": int, "y": int}, total=False)

# invalid identifiers
MyType9 = TypedDict("MyType9", {"in": int, "x-y": int})

# using Literal type
MyType10 = TypedDict("MyType10", {"key": Literal["value"]})

# using namespace TypedDict
MyType11 = typing.TypedDict("MyType11", {"key": int})

# unpacking
c = {"c": float}
MyType12 = TypedDict("MyType1", {"a": int, "b": str, **c})
