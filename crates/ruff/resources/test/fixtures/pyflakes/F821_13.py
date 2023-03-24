"""Test case: ForwardRef."""

from typing import ForwardRef, TypeVar

X = ForwardRef("List[int]")
Y: ForwardRef("List[int]")

Z = TypeVar("X", "List[int]", "int")
