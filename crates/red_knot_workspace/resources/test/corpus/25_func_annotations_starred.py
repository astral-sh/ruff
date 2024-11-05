# Regression test for https://github.com/astral-sh/ruff/issues/14115
#
# This is invalid syntax, but should not lead to a crash.

def f() -> *int: ...


f()
