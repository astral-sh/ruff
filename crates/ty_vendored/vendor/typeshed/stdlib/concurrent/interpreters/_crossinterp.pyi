import sys
from collections.abc import Callable
from typing import Final, NewType
from typing_extensions import Never, Self, TypeAlias

if sys.version_info >= (3, 13):  # needed to satisfy pyright checks for Python <3.13
    from _interpqueues import _UnboundOp

    class ItemInterpreterDestroyed(Exception): ...
    # Actually a descriptor that behaves similarly to classmethod but prevents
    # access from instances.
    classonly = classmethod

    class UnboundItem:
        __slots__ = ()
        def __new__(cls) -> Never: ...
        @classonly
        def singleton(cls, kind: str, module: str, name: str = "UNBOUND") -> Self: ...

    # Sentinel types and alias that don't exist at runtime.
    _UnboundErrorType = NewType("_UnboundErrorType", object)
    _UnboundRemoveType = NewType("_UnboundRemoveType", object)
    _AnyUnbound: TypeAlias = _UnboundErrorType | _UnboundRemoveType | UnboundItem

    UNBOUND_ERROR: Final[_UnboundErrorType]
    UNBOUND_REMOVE: Final[_UnboundRemoveType]
    UNBOUND: Final[UnboundItem]  # analogous to UNBOUND_REPLACE in C

    def serialize_unbound(unbound: _AnyUnbound) -> tuple[_UnboundOp]: ...
    def resolve_unbound(flag: _UnboundOp, exctype_destroyed: Callable[[str], BaseException]) -> UnboundItem: ...
