"""Regression fixture for D418 (`OverloadWithDocstring`) on stub files.

D418 should not fire in stub files (`.pyi`). Stubs have no non-overloaded
implementation that could carry a shared docstring, so the docstring must
live on one of the overloads.

See https://github.com/astral-sh/ruff/issues/26153.
"""

from typing import overload


@overload
def overloaded_func(a: int) -> str: ...
@overload
def overloaded_func(a: str) -> str:
    """Return the string form of ``a``."""


class C:
    @overload
    def overloaded_method(self, a: int) -> str: ...
    @overload
    def overloaded_method(self, a: str) -> str:
        """Return the string form of ``a``."""


# Nested function with `@overload`, also inside a stub file.
def outer() -> None:
    @overload
    def inner(a: int) -> str: ...
    @overload
    def inner(a: str) -> str:
        """Return the string form of ``a``."""
