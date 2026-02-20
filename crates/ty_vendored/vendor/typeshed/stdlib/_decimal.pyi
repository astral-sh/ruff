"""C decimal arithmetic module"""

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

ROUND_DOWN: Final = "ROUND_DOWN"
ROUND_HALF_UP: Final = "ROUND_HALF_UP"
ROUND_HALF_EVEN: Final = "ROUND_HALF_EVEN"
ROUND_CEILING: Final = "ROUND_CEILING"
ROUND_FLOOR: Final = "ROUND_FLOOR"
ROUND_UP: Final = "ROUND_UP"
ROUND_HALF_DOWN: Final = "ROUND_HALF_DOWN"
ROUND_05UP: Final = "ROUND_05UP"
HAVE_CONTEXTVAR: Final[bool]
HAVE_THREADS: Final[bool]
MAX_EMAX: Final[int]
MAX_PREC: Final[int]
MIN_EMIN: Final[int]
MIN_ETINY: Final[int]
if sys.version_info >= (3, 14):
    IEEE_CONTEXT_MAX_BITS: Final[int]

def setcontext(context: Context, /) -> None:
    """Set a new default context."""

def getcontext() -> Context:
    """Get the current default context."""

if sys.version_info >= (3, 11):
    def localcontext(
        ctx: Context | None = None,
        *,
        prec: int | None = None,
        rounding: str | None = None,
        Emin: int | None = None,
        Emax: int | None = None,
        capitals: int | None = None,
        clamp: int | None = None,
        traps: dict[_TrapType, bool] | None = None,
        flags: dict[_TrapType, bool] | None = None,
    ) -> _ContextManager:
        """Return a context manager that will set the default context to a copy of ctx
        on entry to the with-statement and restore the previous default context when
        exiting the with-statement. If no context is specified, a copy of the current
        default context is used.

        """

else:
    def localcontext(ctx: Context | None = None) -> _ContextManager:
        """Return a context manager that will set the default context to a copy of ctx
        on entry to the with-statement and restore the previous default context when
        exiting the with-statement. If no context is specified, a copy of the current
        default context is used.

        """

if sys.version_info >= (3, 14):
    def IEEEContext(bits: int, /) -> Context:
        """Return a context object initialized to the proper values for one of the
        IEEE interchange formats.  The argument must be a multiple of 32 and less
        than IEEE_CONTEXT_MAX_BITS.

        """

DefaultContext: Context
BasicContext: Context
ExtendedContext: Context
