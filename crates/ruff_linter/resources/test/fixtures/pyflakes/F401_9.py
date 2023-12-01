"""Test: late-binding of `__all__`."""

__all__ = ("bar",)
from foo import bar, baz
