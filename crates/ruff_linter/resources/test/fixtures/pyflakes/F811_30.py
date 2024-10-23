"""Regression test for: https://github.com/astral-sh/ruff/issues/11828"""


class A:
    """A."""

    def foo(self) -> None:
        """Foo."""

    bar = foo

    def bar(self) -> None:
        """Bar."""


class B:
    """B."""
    def baz(self) -> None:
        """Baz."""

    baz = 1


class C:
    """C."""
    def foo(self) -> None:
        """Foo."""

    bar = (foo := 1)


class D:
    """D."""
    foo = 1
    foo = 2
    bar = (foo := 3)
    bar = (foo := 4)
