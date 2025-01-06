import sys
from decimal import (
    Clamped as Clamped,
    Context as Context,
    ConversionSyntax as ConversionSyntax,
    Decimal as Decimal,
    DecimalException as DecimalException,
    DecimalTuple as DecimalTuple,
    DivisionByZero as DivisionByZero,
    DivisionImpossible as DivisionImpossible,
    DivisionUndefined as DivisionUndefined,
    FloatOperation as FloatOperation,
    Inexact as Inexact,
    InvalidContext as InvalidContext,
    InvalidOperation as InvalidOperation,
    Overflow as Overflow,
    Rounded as Rounded,
    Subnormal as Subnormal,
    Underflow as Underflow,
    _ContextManager,
)
from typing import Final
from typing_extensions import TypeAlias

_TrapType: TypeAlias = type[DecimalException]

__version__: Final[str]
__libmpdec_version__: Final[str]

ROUND_DOWN: Final[str]
ROUND_HALF_UP: Final[str]
ROUND_HALF_EVEN: Final[str]
ROUND_CEILING: Final[str]
ROUND_FLOOR: Final[str]
ROUND_UP: Final[str]
ROUND_HALF_DOWN: Final[str]
ROUND_05UP: Final[str]
HAVE_CONTEXTVAR: Final[bool]
HAVE_THREADS: Final[bool]
MAX_EMAX: Final[int]
MAX_PREC: Final[int]
MIN_EMIN: Final[int]
MIN_ETINY: Final[int]

def setcontext(context: Context, /) -> None: ...
def getcontext() -> Context: ...

if sys.version_info >= (3, 11):
    def localcontext(
        ctx: Context | None = None,
        *,
        prec: int | None = ...,
        rounding: str | None = ...,
        Emin: int | None = ...,
        Emax: int | None = ...,
        capitals: int | None = ...,
        clamp: int | None = ...,
        traps: dict[_TrapType, bool] | None = ...,
        flags: dict[_TrapType, bool] | None = ...,
    ) -> _ContextManager: ...

else:
    def localcontext(ctx: Context | None = None) -> _ContextManager: ...

DefaultContext: Context
BasicContext: Context
ExtendedContext: Context
