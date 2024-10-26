"""Regression test for: https://github.com/astral-sh/ruff/issues/13930"""

from queue import Empty

class Types:
    INVALID = 0
    UINT = 1
    HEX = 2
    Empty = 3
