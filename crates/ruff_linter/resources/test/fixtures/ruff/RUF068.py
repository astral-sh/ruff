import typing


class A: ...


class B: ...


# Good
__all__ = "A" + "B"
__all__: list[str] = ["A", "B"]
__all__: typing.Any = ("A", "B")
__all__ = ["A", "B"]
__all__ = [A, "B", "B"]
__all__ += ["A", "B"]
__all__.extend(["A", "B"])

# Bad
__all__: list[str] = ["A", "B", "A"]
__all__: typing.Any = ("A", "B", "B")
__all__ = ["A", "B", "A"]
__all__ = ["A", "A", "B", "B"]
__all__ = [
    "A",
    "A",
    "B",
    "B"
]
__all__ += ["B", "B"]
__all__.extend(["B", "B"])

# Bad, unsafe
__all__ = [
    "A",
    "A",
    "B",
    # Comment
    "B",  # 2
    # 3
]
