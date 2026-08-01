"""Regression test for: https://github.com/astral-sh/ruff/issues/24915"""

from foo import bar


class Foo:
    def bar(self): ...

    def bar(self): ...


def bar(): ...
