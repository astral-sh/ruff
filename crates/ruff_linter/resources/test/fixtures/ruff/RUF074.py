# Errors: single-line __all__ with multiple items

__all__ = ["a", "b", "c"]
__all__ = ("a", "b", "c")
__all__ += ["x", "y"]
__all__: list[str] = ["foo", "bar", "baz"]
__all__.extend(["one", "two"])
__all__.extend(("one", "two"))

# Non-string items are also flagged (formatting rule)
__all__ = [foo, bar]

# Nested indentation context
if True:
    __all__ = ["a", "b", "c"]
    __all__ += ["d", "e"]

# OK: already multiline

__all__ = [
    "a",
    "b",
    "c",
]

__all__ = (
    "a",
    "b",
    "c",
)

# OK: single item (no need to enforce multiline)

__all__ = ["a"]
__all__ = ("a",)

# OK: empty

__all__ = []
__all__ = ()

# OK: not __all__

__slots__ = ["a", "b", "c"]
other = ["a", "b", "c"]

# OK: not in module scope

def func():
    __all__ = ["a", "b", "c"]

class Cls:
    __all__ = ["a", "b", "c"]

# OK: unparenthesized tuple (cannot safely expand)
__all__ = "a", "b", "c"
