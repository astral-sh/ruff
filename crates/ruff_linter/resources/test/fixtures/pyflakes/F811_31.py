"""Regression test for: https://github.com/astral-sh/ruff/issues/12309"""

import contextlib

foo = None
with contextlib.suppress(ImportError):
    from some_module import foo

bar = None
try:
    from some_module import bar
except ImportError:
    pass


try:
    baz = None

    from some_module import baz
except ImportError:
    pass
