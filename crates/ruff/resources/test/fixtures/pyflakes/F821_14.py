"""Test case: f-strings in type annotations."""

from typing import List

x = 1

x: List[f"i{x}nt"] = []
