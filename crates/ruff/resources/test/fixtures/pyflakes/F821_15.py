"""Test case: f-strings in future type annotations."""

from __future__ import annotations

from typing import List

x = 1

x: List[f"i{x}nt"] = []
