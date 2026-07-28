# The fix must not use `Union` from the conditional import.
# https://github.com/astral-sh/ruff/issues/4419
if False:
    from typing import Union


def func(x):
    if x:
        return 1
    return "a"
