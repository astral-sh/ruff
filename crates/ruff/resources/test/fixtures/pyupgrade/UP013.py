from typing import TypedDict, NotRequired, Literal
import typing

# dict literal
MyType = TypedDict("MyType", {"a": int, "b": str})

# dict call
MyType = TypedDict("MyType", dict(a=int, b=str))

# kwargs
MyType = TypedDict("MyType", a=int, b=str)

# Empty TypedDict
MyType = TypedDict("MyType")

# Literal values
MyType = TypedDict("MyType", {"a": "hello"})
MyType = TypedDict("MyType", a="hello")

# NotRequired
MyType = TypedDict("MyType", {"a": NotRequired[dict]})

# total
MyType = TypedDict("MyType", {"x": int, "y": int}, total=False)

# using Literal type
MyType = TypedDict("MyType", {"key": Literal["value"]})

# using namespace TypedDict
MyType = typing.TypedDict("MyType", {"key": int})

# invalid identifiers (OK)
MyType = TypedDict("MyType", {"in": int, "x-y": int})

# unpacking (OK)
c = {"c": float}
MyType = TypedDict("MyType", {"a": int, "b": str, **c})

# Empty dict literal
MyType = TypedDict("MyType", {})

# Empty dict call
MyType = TypedDict("MyType", dict())
