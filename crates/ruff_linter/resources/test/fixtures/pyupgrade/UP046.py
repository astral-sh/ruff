from collections.abc import Callable
from typing import Generic, ParamSpec, TypeVar, TypeVarTuple

S = TypeVar("S", str, bytes)  # constrained type variable
T = TypeVar("T", bound=float)
Ts = TypeVarTuple("Ts")
P = ParamSpec("P")


class A(Generic[T]):
    # Comments in a class body are preserved
    pass


class B(Generic[*Ts]):
    pass


class C(Generic[P]):
    pass


class Constrained(Generic[S]):
    pass


def f(t: T):
    pass


def g(ts: tuple[*Ts]):
    pass


def h(
    p: Callable[P, T],
    # Comment in the middle of a parameter list should be preserved
    another_param,
    and_another,
):
    pass


def i(s: S):
    pass


# These cases are not handled
class D(Generic[T, T]):  # duplicate generic variable, runtime error
    pass


# TODO(brent) we should also apply the fix to methods, but it will need a
# little more work. these should be left alone for now but be fixed eventually.
class NotGeneric:
    # -> generic_method[T: float](t: T)
    def generic_method(t: T):
        pass


# This one is strange in particular because of the mix of old- and new-style
# generics, but according to the PEP, this is okay "if the class, function, or
# type alias does not use the new syntax." `more_generic` doesn't use the new
# syntax, so it can use T from the module and U from the class scope.
class MixedGenerics[U]:
    def more_generic(u: U, t: T):
        pass


# TODO(brent) we should also handle multiple base classes
class Multiple(NotGeneric, Generic[T]):
    pass
