# The import and the call are in the same branch, so the fix can use it.
# https://github.com/astral-sh/ruff/issues/4419
if cond:
    import sys

    exit(1)
