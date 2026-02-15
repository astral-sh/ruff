import sys
from _operator import (
    abs as abs,
    add as add,
    and_ as and_,
    concat as concat,
    contains as contains,
    countOf as countOf,
    delitem as delitem,
    eq as eq,
    floordiv as floordiv,
    ge as ge,
    getitem as getitem,
    gt as gt,
    iadd as iadd,
    iand as iand,
    iconcat as iconcat,
    ifloordiv as ifloordiv,
    ilshift as ilshift,
    imatmul as imatmul,
    imod as imod,
    imul as imul,
    index as index,
    indexOf as indexOf,
    inv as inv,
    invert as invert,
    ior as ior,
    ipow as ipow,
    irshift as irshift,
    is_ as is_,
    is_not as is_not,
    isub as isub,
    itruediv as itruediv,
    ixor as ixor,
    le as le,
    length_hint as length_hint,
    lshift as lshift,
    lt as lt,
    matmul as matmul,
    mod as mod,
    mul as mul,
    ne as ne,
    neg as neg,
    not_ as not_,
    or_ as or_,
    pos as pos,
    pow as pow,
    rshift as rshift,
    setitem as setitem,
    sub as sub,
    truediv as truediv,
    truth as truth,
    xor as xor,
)
from _typeshed import SupportsGetItem
from typing import Any, Generic, TypeVar, final, overload
from typing_extensions import Self, TypeVarTuple, Unpack

_T = TypeVar("_T")
_T_co = TypeVar("_T_co", covariant=True)
_T1 = TypeVar("_T1")
_T2 = TypeVar("_T2")
_Ts = TypeVarTuple("_Ts")

__all__ = [
    "abs",
    "add",
    "and_",
    "attrgetter",
    "concat",
    "contains",
    "countOf",
    "delitem",
    "eq",
    "floordiv",
    "ge",
    "getitem",
    "gt",
    "iadd",
    "iand",
    "iconcat",
    "ifloordiv",
    "ilshift",
    "imatmul",
    "imod",
    "imul",
    "index",
    "indexOf",
    "inv",
    "invert",
    "ior",
    "ipow",
    "irshift",
    "is_",
    "is_not",
    "isub",
    "itemgetter",
    "itruediv",
    "ixor",
    "le",
    "length_hint",
    "lshift",
    "lt",
    "matmul",
    "methodcaller",
    "mod",
    "mul",
    "ne",
    "neg",
    "not_",
    "or_",
    "pos",
    "pow",
    "rshift",
    "setitem",
    "sub",
    "truediv",
    "truth",
    "xor",
]

if sys.version_info >= (3, 11):
    from _operator import call as call

    __all__ += ["call"]

if sys.version_info >= (3, 14):
    from _operator import is_none as is_none, is_not_none as is_not_none

    __all__ += ["is_none", "is_not_none"]

__lt__ = lt
__le__ = le
__eq__ = eq
__ne__ = ne
__ge__ = ge
__gt__ = gt
__not__ = not_
__abs__ = abs
__add__ = add
__and__ = and_
__floordiv__ = floordiv
__index__ = index
__inv__ = inv
__invert__ = invert
__lshift__ = lshift
__mod__ = mod
__mul__ = mul
__matmul__ = matmul
__neg__ = neg
__or__ = or_
__pos__ = pos
__pow__ = pow
__rshift__ = rshift
__sub__ = sub
__truediv__ = truediv
__xor__ = xor
__concat__ = concat
__contains__ = contains
__delitem__ = delitem
__getitem__ = getitem
__setitem__ = setitem
__iadd__ = iadd
__iand__ = iand
__iconcat__ = iconcat
__ifloordiv__ = ifloordiv
__ilshift__ = ilshift
__imod__ = imod
__imul__ = imul
__imatmul__ = imatmul
__ior__ = ior
__ipow__ = ipow
__irshift__ = irshift
__isub__ = isub
__itruediv__ = itruediv
__ixor__ = ixor
if sys.version_info >= (3, 11):
    __call__ = call

# At runtime, these classes are implemented in C as part of the _operator module
# However, they consider themselves to live in the operator module, so we'll put
# them here.
@final
class attrgetter(Generic[_T_co]):
    @overload
    def __new__(cls, attr: str, /) -> attrgetter[Any]: ...
    @overload
    def __new__(cls, attr: str, attr2: str, /) -> attrgetter[tuple[Any, Any]]: ...
    @overload
    def __new__(cls, attr: str, attr2: str, attr3: str, /) -> attrgetter[tuple[Any, Any, Any]]: ...
    @overload
    def __new__(cls, attr: str, attr2: str, attr3: str, attr4: str, /) -> attrgetter[tuple[Any, Any, Any, Any]]: ...
    @overload
    def __new__(cls, attr: str, /, *attrs: str) -> attrgetter[tuple[Any, ...]]: ...
    def __call__(self, obj: Any, /) -> _T_co: ...

@final
class itemgetter(Generic[_T_co]):
    @overload
    def __new__(cls, item: _T, /) -> itemgetter[_T]: ...
    @overload
    def __new__(cls, item1: _T1, item2: _T2, /, *items: Unpack[_Ts]) -> itemgetter[tuple[_T1, _T2, Unpack[_Ts]]]: ...
    # __key: _KT_contra in SupportsGetItem seems to be causing variance issues, ie:
    # TypeVar "_KT_contra@SupportsGetItem" is contravariant
    #   "tuple[int, int]" is incompatible with protocol "SupportsIndex"
    # preventing [_T_co, ...] instead of [Any, ...]
    #
    # If we can't infer a literal key from __new__ (ie: `itemgetter[Literal[0]]` for `itemgetter(0)`),
    # then we can't annotate __call__'s return type or it'll break on tuples
    #
    # These issues are best demonstrated by the `itertools.check_itertools_recipes.unique_justseen` test.
    def __call__(self, obj: SupportsGetItem[Any, Any]) -> Any: ...

@final
class methodcaller:
    def __new__(cls, name: str, /, *args: Any, **kwargs: Any) -> Self: ...
    def __call__(self, obj: Any) -> Any: ...
