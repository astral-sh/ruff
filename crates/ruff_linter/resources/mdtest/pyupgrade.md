# `non-pep695-generic-class` (`UP046`)

<!-- snapshot-diagnostics -->

```toml
target-version = "py312"
lint.select = ["UP046"]
```

```py
from typing import Any, AnyStr, Generic, ParamSpec, TypeVar, TypeVarTuple

from somewhere import SupportsRichComparisonT

S = TypeVar("S", str, bytes)  # constrained type variable
T = TypeVar("T", bound=float)
Ts = TypeVarTuple("Ts")
P = ParamSpec("P")


class A(Generic[T]):  # error: [non-pep695-generic-class]
    # Comments in a class body are preserved
    var: T


class B(Generic[*Ts]):  # error: [non-pep695-generic-class]
    var: tuple[*Ts]


class C(Generic[P]):  # error: [non-pep695-generic-class]
    var: P


class Constrained(Generic[S]):  # error: [non-pep695-generic-class]
    var: S


# This case gets a diagnostic but not a fix because we can't look up the bounds
# or constraints on the TypeVar imported from another module
class ExternalType(Generic[T, SupportsRichComparisonT]):  # error: [non-pep695-generic-class]
    var: T
    compare: SupportsRichComparisonT


# typing.AnyStr is a common external type variable, so treat it specially as a
# known TypeVar
class MyStr(Generic[AnyStr]):  # error: [non-pep695-generic-class]
    s: AnyStr


class MultipleGenerics(Generic[S, T, *Ts, P]):  # error: [non-pep695-generic-class]
    var: S
    typ: T
    tup: tuple[*Ts]
    pep: P


class MultipleBaseClasses(list, Generic[T]):  # error: [non-pep695-generic-class]
    var: T


# these are just for the MoreBaseClasses and MultipleBaseAndGenerics cases
class Base1: ...


class Base2: ...


class Base3: ...


class MoreBaseClasses(Base1, Base2, Base3, Generic[T]):  # error: [non-pep695-generic-class]
    var: T


class MultipleBaseAndGenerics(Base1, Base2, Base3, Generic[S, T, *Ts, P]):  # error: [non-pep695-generic-class]
    var: S
    typ: T
    tup: tuple[*Ts]
    pep: P


class A(Generic[T]): ...  # error: [non-pep695-generic-class]


class B(A[S], Generic[S]):  # error: [non-pep695-generic-class]
    var: S


class C(A[S], Generic[S, T]):  # error: [non-pep695-generic-class]
    var: tuple[S, T]


class D(A[int], Generic[T]):  # error: [non-pep695-generic-class]
    var: T


class NotLast(Generic[T], Base1):  # error: [non-pep695-generic-class]
    var: T


class Sandwich(Base1, Generic[T], Base2):  # error: [non-pep695-generic-class]
    var: T


# runtime `TypeError` to inherit from `Generic` multiple times, but we still
# emit a diagnostic
class TooManyGenerics(Generic[T], Generic[S]):  # error: [non-pep695-generic-class]
    var: T
    var: S


# These cases are not handled
class D(Generic[T, T]):  # duplicate generic variable, runtime error
    pass


# TODO(brent) we should also apply the fix to methods, but it will need a
# little more work. these should be left alone for now but be fixed eventually.
class NotGeneric:
    # -> generic_method[T: float](t: T)
    def generic_method(t: T) -> T:
        return t


# This one is strange in particular because of the mix of old- and new-style
# generics, but according to the PEP, this is okay "if the class, function, or
# type alias does not use the new syntax." `more_generic` doesn't use the new
# syntax, so it can use T from the module and U from the class scope.
class MixedGenerics[U]:
    def more_generic(u: U, t: T) -> tuple[U, T]:
        return (u, t)


# default requires 3.13
V = TypeVar("V", default=Any, bound=str)


class DefaultTypeVar(Generic[V]):  # -> [V: str = Any]
    var: V


# Test case for TypeVar with default but no bound
W = TypeVar("W", default=int)


class DefaultOnlyTypeVar(Generic[W]):  # -> [W = int]
    var: W


# nested classes and functions are skipped
class Outer:
    class Inner(Generic[T]):
        var: T
```
