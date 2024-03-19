"""Tests for constructs allowed when `__future__` annotations are enabled but not otherwise"""
from __future__ import annotations

from typing import Optional, TypeAlias, Union

__version__: str
__author__: str

# References to a class from inside the class:
class C:
    other: C = ...  # valid when `__future__.annotations are enabled
    other2: "C" = ...  # always okay
    def from_str(self, s: str) -> C: ...  # valid when `__future__.annotations are enabled
    def from_str2(self, s: str) -> "C": ...  # always okay

# Circular references:
class A:
    foo: B  # valid when `__future__.annotations are enabled
    foo2: "B"  # always okay
    bar: dict[str, B]  # valid when `__future__.annotations are enabled
    bar2: dict[str, "A"]  # always okay

class B:
    foo: A  # always okay
    bar: dict[str, A]  # always okay

# Annotations are treated as assignments in .pyi files, but not in .py files
class MyClass:
    foo: int
    bar = foo  # Still invalid even when `__future__.annotations` are enabled
    bar = "foo"  # always okay

baz: MyClass
eggs = baz  # Still invalid even when `__future__.annotations` are enabled
eggs = "baz"  # always okay

# Forward references:
MaybeDStr: TypeAlias = Optional[DStr]  # Still invalid even when `__future__.annotations` are enabled
MaybeDStr2: TypeAlias = Optional["DStr"]  # always okay
DStr: TypeAlias = Union[D, str]  # Still invalid even when `__future__.annotations` are enabled
DStr2: TypeAlias = Union["D", str]  # always okay

class D: ...

# More circular references
class Leaf: ...
class Tree(list[Tree | Leaf]): ...  # Still invalid even when `__future__.annotations` are enabled
class Tree2(list["Tree | Leaf"]): ...  # always okay
