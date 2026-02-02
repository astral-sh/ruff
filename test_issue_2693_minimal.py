"""Minimal reproduction for ty issue #2693.

The issue: When a base class has overloads with constrained `self` types,
and a child class is generic over the type parameter that the self-constraint
uses, ty doesn't realize that the constrained overload only applies when
the type parameter matches the constraint.
"""
from typing import Any, Generic, TypeVar, overload

T = TypeVar("T")


class Base(Generic[T]):
    @overload
    def method(self, arg: T) -> None: ...
    @overload
    def method(self: "Base[str]", arg: str, extra: str) -> None: ...
    def method(self, arg: Any, extra: str = "") -> None:
        pass


class Child(Base[T]):
    # ty error: Invalid override of method `method` from `Base`
    # This should be valid: when T=str, the str-constrained overload applies
    # and this signature is compatible. When T!=str, only the first overload
    # applies, and this is also compatible.
    def method(self, arg: T, extra: str = "") -> None:
        pass
