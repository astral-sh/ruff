"""Operator interface.

This module exports a set of functions implemented in C corresponding
to the intrinsic operators of Python.  For example, operator.add(x, y)
is equivalent to the expression x+y.  The function names are those
used for special methods; variants without leading and trailing
'__' are also provided for convenience.
"""

import sys
from _typeshed import SupportsGetItem
from collections.abc import Callable, Container, Iterable, MutableMapping, MutableSequence, Sequence
from operator import attrgetter as attrgetter, itemgetter as itemgetter, methodcaller as methodcaller
from typing import Any, AnyStr, Protocol, SupportsAbs, SupportsIndex, TypeVar, overload, type_check_only
from typing_extensions import ParamSpec, TypeAlias, TypeIs

_R = TypeVar("_R")
_T = TypeVar("_T")
_T_co = TypeVar("_T_co", covariant=True)
_K = TypeVar("_K")
_V = TypeVar("_V")
_P = ParamSpec("_P")

# The following protocols return "Any" instead of bool, since the comparison
# operators can be overloaded to return an arbitrary object. For example,
# the numpy.array comparison dunders return another numpy.array.

@type_check_only
class _SupportsDunderLT(Protocol):
    def __lt__(self, other: Any, /) -> Any: ...

@type_check_only
class _SupportsDunderGT(Protocol):
    def __gt__(self, other: Any, /) -> Any: ...

@type_check_only
class _SupportsDunderLE(Protocol):
    def __le__(self, other: Any, /) -> Any: ...

@type_check_only
class _SupportsDunderGE(Protocol):
    def __ge__(self, other: Any, /) -> Any: ...

_SupportsComparison: TypeAlias = _SupportsDunderLE | _SupportsDunderGE | _SupportsDunderGT | _SupportsDunderLT

@type_check_only
class _SupportsInversion(Protocol[_T_co]):
    def __invert__(self) -> _T_co: ...

@type_check_only
class _SupportsNeg(Protocol[_T_co]):
    def __neg__(self) -> _T_co: ...

@type_check_only
class _SupportsPos(Protocol[_T_co]):
    def __pos__(self) -> _T_co: ...

# All four comparison functions must have the same signature, or we get false-positive errors
def lt(a: _SupportsComparison, b: _SupportsComparison, /) -> Any:
    """Same as a < b."""

def le(a: _SupportsComparison, b: _SupportsComparison, /) -> Any:
    """Same as a <= b."""

def eq(a: object, b: object, /) -> Any:
    """Same as a == b."""

def ne(a: object, b: object, /) -> Any:
    """Same as a != b."""

def ge(a: _SupportsComparison, b: _SupportsComparison, /) -> Any:
    """Same as a >= b."""

def gt(a: _SupportsComparison, b: _SupportsComparison, /) -> Any:
    """Same as a > b."""

def not_(a: object, /) -> bool:
    """Same as not a."""

def truth(a: object, /) -> bool:
    """Return True if a is true, False otherwise."""

def is_(a: object, b: object, /) -> bool:
    """Same as a is b."""

def is_not(a: object, b: object, /) -> bool:
    """Same as a is not b."""

def abs(a: SupportsAbs[_T], /) -> _T:
    """Same as abs(a)."""

def add(a: Any, b: Any, /) -> Any:
    """Same as a + b."""

def and_(a: Any, b: Any, /) -> Any:
    """Same as a & b."""

def floordiv(a: Any, b: Any, /) -> Any:
    """Same as a // b."""

def index(a: SupportsIndex, /) -> int:
    """Same as a.__index__()"""

def inv(a: _SupportsInversion[_T_co], /) -> _T_co:
    """Same as ~a."""

def invert(a: _SupportsInversion[_T_co], /) -> _T_co:
    """Same as ~a."""

def lshift(a: Any, b: Any, /) -> Any:
    """Same as a << b."""

def mod(a: Any, b: Any, /) -> Any:
    """Same as a % b."""

def mul(a: Any, b: Any, /) -> Any:
    """Same as a * b."""

def matmul(a: Any, b: Any, /) -> Any:
    """Same as a @ b."""

def neg(a: _SupportsNeg[_T_co], /) -> _T_co:
    """Same as -a."""

def or_(a: Any, b: Any, /) -> Any:
    """Same as a | b."""

def pos(a: _SupportsPos[_T_co], /) -> _T_co:
    """Same as +a."""

def pow(a: Any, b: Any, /) -> Any:
    """Same as a ** b."""

def rshift(a: Any, b: Any, /) -> Any:
    """Same as a >> b."""

def sub(a: Any, b: Any, /) -> Any:
    """Same as a - b."""

def truediv(a: Any, b: Any, /) -> Any:
    """Same as a / b."""

def xor(a: Any, b: Any, /) -> Any:
    """Same as a ^ b."""

def concat(a: Sequence[_T], b: Sequence[_T], /) -> Sequence[_T]:
    """Same as a + b, for a and b sequences."""

def contains(a: Container[object], b: object, /) -> bool:
    """Same as b in a (note reversed operands)."""

def countOf(a: Iterable[object], b: object, /) -> int:
    """Return the number of items in a which are, or which equal, b."""

@overload
def delitem(a: MutableSequence[Any], b: SupportsIndex, /) -> None:
    """Same as del a[b]."""

@overload
def delitem(a: MutableSequence[Any], b: slice, /) -> None: ...
@overload
def delitem(a: MutableMapping[_K, Any], b: _K, /) -> None: ...
@overload
def getitem(a: Sequence[_T], b: slice, /) -> Sequence[_T]:
    """Same as a[b]."""

@overload
def getitem(a: SupportsGetItem[_K, _V], b: _K, /) -> _V: ...
def indexOf(a: Iterable[_T], b: _T, /) -> int:
    """Return the first index of b in a."""

@overload
def setitem(a: MutableSequence[_T], b: SupportsIndex, c: _T, /) -> None:
    """Same as a[b] = c."""

@overload
def setitem(a: MutableSequence[_T], b: slice, c: Sequence[_T], /) -> None: ...
@overload
def setitem(a: MutableMapping[_K, _V], b: _K, c: _V, /) -> None: ...
def length_hint(obj: object, default: int = 0, /) -> int:
    """Return an estimate of the number of items in obj.

    This is useful for presizing containers when building from an iterable.

    If the object supports len(), the result will be exact.
    Otherwise, it may over- or under-estimate by an arbitrary amount.
    The result will be an integer >= 0.
    """

def iadd(a: Any, b: Any, /) -> Any:
    """Same as a += b."""

def iand(a: Any, b: Any, /) -> Any:
    """Same as a &= b."""

def iconcat(a: Any, b: Any, /) -> Any:
    """Same as a += b, for a and b sequences."""

def ifloordiv(a: Any, b: Any, /) -> Any:
    """Same as a //= b."""

def ilshift(a: Any, b: Any, /) -> Any:
    """Same as a <<= b."""

def imod(a: Any, b: Any, /) -> Any:
    """Same as a %= b."""

def imul(a: Any, b: Any, /) -> Any:
    """Same as a *= b."""

def imatmul(a: Any, b: Any, /) -> Any:
    """Same as a @= b."""

def ior(a: Any, b: Any, /) -> Any:
    """Same as a |= b."""

def ipow(a: Any, b: Any, /) -> Any:
    """Same as a **= b."""

def irshift(a: Any, b: Any, /) -> Any:
    """Same as a >>= b."""

def isub(a: Any, b: Any, /) -> Any:
    """Same as a -= b."""

def itruediv(a: Any, b: Any, /) -> Any:
    """Same as a /= b."""

def ixor(a: Any, b: Any, /) -> Any:
    """Same as a ^= b."""

if sys.version_info >= (3, 11):
    def call(obj: Callable[_P, _R], /, *args: _P.args, **kwargs: _P.kwargs) -> _R:
        """Same as obj(*args, **kwargs)."""

def _compare_digest(a: AnyStr, b: AnyStr, /) -> bool:
    """Return 'a == b'.

    This function uses an approach designed to prevent
    timing analysis, making it appropriate for cryptography.

    a and b must both be of the same type: either str (ASCII only),
    or any bytes-like object.

    Note: If a and b are of different lengths, or if an error occurs,
    a timing attack could theoretically reveal information about the
    types and lengths of a and b--but not their values.
    """

if sys.version_info >= (3, 14):
    def is_none(a: object, /) -> TypeIs[None]:
        """Same as a is None."""

    def is_not_none(a: _T | None, /) -> TypeIs[_T]:
        """Same as a is not None."""
