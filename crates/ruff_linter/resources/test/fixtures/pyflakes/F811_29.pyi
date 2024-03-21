"""Regression test for: https://github.com/astral-sh/ruff/issues/10509"""

from foo import Bar as Bar

class Eggs:
    Bar: int  # OK

Bar = 1  # F811
