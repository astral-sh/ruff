# Regression test for https://github.com/astral-sh/ruff/issues/17792

from __future__ import annotations


class C: ...


def f(arg: C):
    pass


x, _ = f(1)

assert x
