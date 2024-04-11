"""Tests for constructs allowed in `.pyi` stub files but not at runtime"""

from typing import Generic, NewType, Optional, TypeAlias, TypeVar, Union

__version__: str
__author__: str

# Forward references:
MaybeCStr: TypeAlias = Optional[CStr]  # valid in a `.pyi` stub file, not in a `.py` runtime file
MaybeCStr2: TypeAlias = Optional["CStr"]  # always okay
CStr: TypeAlias = Union[C, str]  # valid in a `.pyi` stub file, not in a `.py` runtime file
CStr2: TypeAlias = Union["C", str]  # always okay

# References to a class from inside the class:
class C:
    other: C = ...  # valid in a `.pyi` stub file, not in a `.py` runtime file
    other2: "C" = ...  # always okay
    def from_str(self, s: str) -> C: ...  # valid in a `.pyi` stub file, not in a `.py` runtime file
    def from_str2(self, s: str) -> "C": ...  # always okay

# Circular references:
class A:
    foo: B  # valid in a `.pyi` stub file, not in a `.py` runtime file
    foo2: "B"  # always okay
    bar: dict[str, B]  # valid in a `.pyi` stub file, not in a `.py` runtime file
    bar2: dict[str, "A"]  # always okay

class B:
    foo: A  # always okay
    bar: dict[str, A]  # always okay

class Leaf: ...
class Tree(list[Tree | Leaf]): ...  # valid in a `.pyi` stub file, not in a `.py` runtime file
class Tree2(list["Tree | Leaf"]): ...  # always okay

# Generic bases can have forward references in stubs
class Foo(Generic[T]): ...
T = TypeVar("T")
class Bar(Foo[Baz]): ...
class Baz: ...

# bases in general can be forward references in stubs
class Eggs(Spam): ...
class Spam: ...

# NewType can have forward references
MyNew = NewType("MyNew", MyClass)

# Annotations are treated as assignments in .pyi files, but not in .py files
class MyClass:
    foo: int
    bar = foo  # valid in a `.pyi` stub file, not in a `.py` runtime file
    bar = "foo"  # always okay

baz: MyClass
eggs = baz  # valid in a `.pyi` stub file, not in a `.py` runtime file
eggs = "baz"  # always okay

class Blah:
    class Blah2(Blah): ...
