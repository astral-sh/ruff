"""Regression test for https://github.com/astral-sh/ruff/issues/15786. This
should be separate from other tests because it shadows the `map` builtin.

This should still get a diagnostic but not a fix that would lead to an error.
"""

from itertools import starmap

map = {}
for _ in starmap(print, zip("A", "12")):
    pass
