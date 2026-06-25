from typing import overload


@overload
def overloaded_func(a: int) -> str: ...
@overload
def overloaded_func(a: str) -> str:
    """Return the string form of ``a``."""
