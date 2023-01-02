from __future__ import annotations

# test case for https://github.com/charliermarsh/ruff/issues/1552
def _():
    x = 0
    list()[x:]
