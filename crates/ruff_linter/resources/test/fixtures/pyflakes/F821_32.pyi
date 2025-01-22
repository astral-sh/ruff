from typing import TypeVar


# https://github.com/astral-sh/ruff/issues/15677
T = TypeVar(
    "T",
                # Visited as:
    C,          # => Type
    bound=C,    # => Type
    default=C,  # => Type
    foo=C       # => Value
)


class C: ...
