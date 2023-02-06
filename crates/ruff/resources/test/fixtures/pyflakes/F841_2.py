from __future__ import annotations


# Test case for https://github.com/charliermarsh/ruff/issues/1552
def f():
    x = 0
    list()[x:]


# Test case for https://github.com/charliermarsh/ruff/issues/2603
def f():
    KeyTupleT = tuple[str, ...]

    keys_checked: set[KeyTupleT] = set()
    return keys_checked
