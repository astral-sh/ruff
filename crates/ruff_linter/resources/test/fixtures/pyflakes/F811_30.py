"""Regression test for: https://github.com/astral-sh/ruff/issues/11828"""


class A:
    """A."""

    def foo(self) -> None:
        """Foo."""

    bar = foo

    def bar(self) -> None:
        """Bar."""
