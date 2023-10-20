"""Test case: `Literal` with `__future__` annotations."""

from __future__ import annotations

from typing import Literal, Final

from typing_extensions import assert_type

CONSTANT: Final = "ns"

assert_type(CONSTANT, Literal["ns"])
