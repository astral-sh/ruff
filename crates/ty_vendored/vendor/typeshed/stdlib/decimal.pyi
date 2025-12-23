"""Decimal fixed-point and floating-point arithmetic.

This is an implementation of decimal floating-point arithmetic based on
the General Decimal Arithmetic Specification:

    http://speleotrove.com/decimal/decarith.html

and IEEE standard 854-1987:

    http://en.wikipedia.org/wiki/IEEE_854-1987

Decimal floating point has finite precision with arbitrarily large bounds.

The purpose of this module is to support arithmetic using familiar
"schoolhouse" rules and to avoid some of the tricky representation
issues associated with binary floating point.  The package is especially
useful for financial applications or for contexts where users have
expectations that are at odds with binary floating point (for instance,
in binary floating point, 1.00 % 0.1 gives 0.09999999999999995 instead
of 0.0; Decimal('1.00') % Decimal('0.1') returns the expected
Decimal('0.00')).

Here are some examples of using the decimal module:

>>> from decimal import *
>>> setcontext(ExtendedContext)
>>> Decimal(0)
Decimal('0')
>>> Decimal('1')
Decimal('1')
>>> Decimal('-.0123')
Decimal('-0.0123')
>>> Decimal(123456)
Decimal('123456')
>>> Decimal('123.45e12345678')
Decimal('1.2345E+12345680')
>>> Decimal('1.33') + Decimal('1.27')
Decimal('2.60')
>>> Decimal('12.34') + Decimal('3.87') - Decimal('18.41')
Decimal('-2.20')
>>> dig = Decimal(1)
>>> print(dig / Decimal(3))
0.333333333
>>> getcontext().prec = 18
>>> print(dig / Decimal(3))
0.333333333333333333
>>> print(dig.sqrt())
1
>>> print(Decimal(3).sqrt())
1.73205080756887729
>>> print(Decimal(3) ** 123)
4.85192780976896427E+58
>>> inf = Decimal(1) / Decimal(0)
>>> print(inf)
Infinity
>>> neginf = Decimal(-1) / Decimal(0)
>>> print(neginf)
-Infinity
>>> print(neginf + inf)
NaN
>>> print(neginf * inf)
-Infinity
>>> print(dig / 0)
Infinity
>>> getcontext().traps[DivisionByZero] = 1
>>> print(dig / 0)
Traceback (most recent call last):
  ...
  ...
  ...
decimal.DivisionByZero: x / 0
>>> c = Context()
>>> c.traps[InvalidOperation] = 0
>>> print(c.flags[InvalidOperation])
0
>>> c.divide(Decimal(0), Decimal(0))
Decimal('NaN')
>>> c.traps[InvalidOperation] = 1
>>> print(c.flags[InvalidOperation])
1
>>> c.flags[InvalidOperation] = 0
>>> print(c.flags[InvalidOperation])
0
>>> print(c.divide(Decimal(0), Decimal(0)))
Traceback (most recent call last):
  ...
  ...
  ...
decimal.InvalidOperation: 0 / 0
>>> print(c.flags[InvalidOperation])
1
>>> c.flags[InvalidOperation] = 0
>>> c.traps[InvalidOperation] = 0
>>> print(c.divide(Decimal(0), Decimal(0)))
NaN
>>> print(c.flags[InvalidOperation])
1
>>>
"""

import numbers
import sys
from _decimal import (
    HAVE_CONTEXTVAR as HAVE_CONTEXTVAR,
    HAVE_THREADS as HAVE_THREADS,
    MAX_EMAX as MAX_EMAX,
    MAX_PREC as MAX_PREC,
    MIN_EMIN as MIN_EMIN,
    MIN_ETINY as MIN_ETINY,
    ROUND_05UP as ROUND_05UP,
    ROUND_CEILING as ROUND_CEILING,
    ROUND_DOWN as ROUND_DOWN,
    ROUND_FLOOR as ROUND_FLOOR,
    ROUND_HALF_DOWN as ROUND_HALF_DOWN,
    ROUND_HALF_EVEN as ROUND_HALF_EVEN,
    ROUND_HALF_UP as ROUND_HALF_UP,
    ROUND_UP as ROUND_UP,
    BasicContext as BasicContext,
    DefaultContext as DefaultContext,
    ExtendedContext as ExtendedContext,
    __libmpdec_version__ as __libmpdec_version__,
    __version__ as __version__,
    getcontext as getcontext,
    localcontext as localcontext,
    setcontext as setcontext,
)
from collections.abc import Container, Sequence
from types import TracebackType
from typing import Any, ClassVar, Literal, NamedTuple, final, overload, type_check_only
from typing_extensions import Self, TypeAlias, disjoint_base

if sys.version_info >= (3, 14):
    from _decimal import IEEE_CONTEXT_MAX_BITS as IEEE_CONTEXT_MAX_BITS, IEEEContext as IEEEContext

_Decimal: TypeAlias = Decimal | int
_DecimalNew: TypeAlias = Decimal | float | str | tuple[int, Sequence[int], int]
_ComparableNum: TypeAlias = Decimal | float | numbers.Rational
_TrapType: TypeAlias = type[DecimalException]

# At runtime, these classes are implemented in C as part of "_decimal".
# However, they consider themselves to live in "decimal", so we'll put them here.

# This type isn't exposed at runtime. It calls itself decimal.ContextManager
@final
@type_check_only
class _ContextManager:
    def __init__(self, new_context: Context) -> None: ...
    def __enter__(self) -> Context: ...
    def __exit__(self, t: type[BaseException] | None, v: BaseException | None, tb: TracebackType | None) -> None: ...

class DecimalTuple(NamedTuple):
    """DecimalTuple(sign, digits, exponent)"""

    sign: int
    digits: tuple[int, ...]
    exponent: int | Literal["n", "N", "F"]

class DecimalException(ArithmeticError): ...
class Clamped(DecimalException): ...
class InvalidOperation(DecimalException): ...
class ConversionSyntax(InvalidOperation): ...
class DivisionByZero(DecimalException, ZeroDivisionError): ...
class DivisionImpossible(InvalidOperation): ...
class DivisionUndefined(InvalidOperation, ZeroDivisionError): ...
class Inexact(DecimalException): ...
class InvalidContext(InvalidOperation): ...
class Rounded(DecimalException): ...
class Subnormal(DecimalException): ...
class Overflow(Inexact, Rounded): ...
class Underflow(Inexact, Rounded, Subnormal): ...
class FloatOperation(DecimalException, TypeError): ...

@disjoint_base
class Decimal:
    """Construct a new Decimal object. 'value' can be an integer, string, tuple,
    or another Decimal object. If no value is given, return Decimal('0'). The
    context does not affect the conversion and is only passed to determine if
    the InvalidOperation trap is active.

    """

    def __new__(cls, value: _DecimalNew = "0", context: Context | None = None) -> Self: ...
    if sys.version_info >= (3, 14):
        @classmethod
        def from_number(cls, number: Decimal | float, /) -> Self:
            """Class method that converts a real number to a decimal number, exactly.

            >>> Decimal.from_number(314)              # int
            Decimal('314')
            >>> Decimal.from_number(0.1)              # float
            Decimal('0.1000000000000000055511151231257827021181583404541015625')
            >>> Decimal.from_number(Decimal('3.14'))  # another decimal instance
            Decimal('3.14')


            """

    @classmethod
    def from_float(cls, f: float, /) -> Self:
        """Class method that converts a float to a decimal number, exactly.
        Since 0.1 is not exactly representable in binary floating point,
        Decimal.from_float(0.1) is not the same as Decimal('0.1').

            >>> Decimal.from_float(0.1)
            Decimal('0.1000000000000000055511151231257827021181583404541015625')
            >>> Decimal.from_float(float('nan'))
            Decimal('NaN')
            >>> Decimal.from_float(float('inf'))
            Decimal('Infinity')
            >>> Decimal.from_float(float('-inf'))
            Decimal('-Infinity')


        """

    def __bool__(self) -> bool:
        """True if self else False"""

    def compare(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Compare self to other.  Return a decimal value:

        a or b is a NaN ==> Decimal('NaN')
        a < b           ==> Decimal('-1')
        a == b          ==> Decimal('0')
        a > b           ==> Decimal('1')

        """

    def __hash__(self) -> int: ...
    def as_tuple(self) -> DecimalTuple:
        """Return a tuple representation of the number."""

    def as_integer_ratio(self) -> tuple[int, int]:
        """Decimal.as_integer_ratio() -> (int, int)

        Return a pair of integers, whose ratio is exactly equal to the original
        Decimal and with a positive denominator. The ratio is in lowest terms.
        Raise OverflowError on infinities and a ValueError on NaNs.

        """

    def to_eng_string(self, context: Context | None = None) -> str:
        """Convert to an engineering-type string.  Engineering notation has an exponent
        which is a multiple of 3, so there are up to 3 digits left of the decimal
        place. For example, Decimal('123E+1') is converted to Decimal('1.23E+3').

        The value of context.capitals determines whether the exponent sign is lower
        or upper case. Otherwise, the context does not affect the operation.

        """

    def __abs__(self) -> Decimal:
        """abs(self)"""

    def __add__(self, value: _Decimal, /) -> Decimal:
        """Return self+value."""

    def __divmod__(self, value: _Decimal, /) -> tuple[Decimal, Decimal]:
        """Return divmod(self, value)."""

    def __eq__(self, value: object, /) -> bool: ...
    def __floordiv__(self, value: _Decimal, /) -> Decimal:
        """Return self//value."""

    def __ge__(self, value: _ComparableNum, /) -> bool: ...
    def __gt__(self, value: _ComparableNum, /) -> bool: ...
    def __le__(self, value: _ComparableNum, /) -> bool: ...
    def __lt__(self, value: _ComparableNum, /) -> bool: ...
    def __mod__(self, value: _Decimal, /) -> Decimal:
        """Return self%value."""

    def __mul__(self, value: _Decimal, /) -> Decimal:
        """Return self*value."""

    def __neg__(self) -> Decimal:
        """-self"""

    def __pos__(self) -> Decimal:
        """+self"""

    def __pow__(self, value: _Decimal, mod: _Decimal | None = None, /) -> Decimal:
        """Return pow(self, value, mod)."""

    def __radd__(self, value: _Decimal, /) -> Decimal:
        """Return value+self."""

    def __rdivmod__(self, value: _Decimal, /) -> tuple[Decimal, Decimal]:
        """Return divmod(value, self)."""

    def __rfloordiv__(self, value: _Decimal, /) -> Decimal:
        """Return value//self."""

    def __rmod__(self, value: _Decimal, /) -> Decimal:
        """Return value%self."""

    def __rmul__(self, value: _Decimal, /) -> Decimal:
        """Return value*self."""

    def __rsub__(self, value: _Decimal, /) -> Decimal:
        """Return value-self."""

    def __rtruediv__(self, value: _Decimal, /) -> Decimal:
        """Return value/self."""

    def __sub__(self, value: _Decimal, /) -> Decimal:
        """Return self-value."""

    def __truediv__(self, value: _Decimal, /) -> Decimal:
        """Return self/value."""

    def remainder_near(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Return the remainder from dividing self by other.  This differs from
        self % other in that the sign of the remainder is chosen so as to minimize
        its absolute value. More precisely, the return value is self - n * other
        where n is the integer nearest to the exact value of self / other, and
        if two integers are equally near then the even one is chosen.

        If the result is zero then its sign will be the sign of self.

        """

    def __float__(self) -> float:
        """float(self)"""

    def __int__(self) -> int:
        """int(self)"""

    def __trunc__(self) -> int: ...
    @property
    def real(self) -> Decimal: ...
    @property
    def imag(self) -> Decimal: ...
    def conjugate(self) -> Decimal:
        """Return self."""

    def __complex__(self) -> complex: ...
    @overload
    def __round__(self) -> int: ...
    @overload
    def __round__(self, ndigits: int, /) -> Decimal: ...
    def __floor__(self) -> int: ...
    def __ceil__(self) -> int: ...
    def fma(self, other: _Decimal, third: _Decimal, context: Context | None = None) -> Decimal:
        """Fused multiply-add.  Return self*other+third with no rounding of the
        intermediate product self*other.

            >>> Decimal(2).fma(3, 5)
            Decimal('11')


        """

    def __rpow__(self, value: _Decimal, mod: Context | None = None, /) -> Decimal:
        """Return pow(value, self, mod)."""

    def normalize(self, context: Context | None = None) -> Decimal:
        """Normalize the number by stripping the rightmost trailing zeros and
        converting any result equal to Decimal('0') to Decimal('0e0').  Used
        for producing canonical values for members of an equivalence class.
        For example, Decimal('32.100') and Decimal('0.321000e+2') both normalize
        to the equivalent value Decimal('32.1').

        """

    def quantize(self, exp: _Decimal, rounding: str | None = None, context: Context | None = None) -> Decimal:
        """Return a value equal to the first operand after rounding and having the
        exponent of the second operand.

            >>> Decimal('1.41421356').quantize(Decimal('1.000'))
            Decimal('1.414')

        Unlike other operations, if the length of the coefficient after the quantize
        operation would be greater than precision, then an InvalidOperation is signaled.
        This guarantees that, unless there is an error condition, the quantized exponent
        is always equal to that of the right-hand operand.

        Also unlike other operations, quantize never signals Underflow, even if the
        result is subnormal and inexact.

        If the exponent of the second operand is larger than that of the first, then
        rounding may be necessary. In this case, the rounding mode is determined by the
        rounding argument if given, else by the given context argument; if neither
        argument is given, the rounding mode of the current thread's context is used.

        """

    def same_quantum(self, other: _Decimal, context: Context | None = None) -> bool:
        """Test whether self and other have the same exponent or whether both are NaN.

        This operation is unaffected by context and is quiet: no flags are changed
        and no rounding is performed. As an exception, the C version may raise
        InvalidOperation if the second operand cannot be converted exactly.

        """

    def to_integral_exact(self, rounding: str | None = None, context: Context | None = None) -> Decimal:
        """Round to the nearest integer, signaling Inexact or Rounded as appropriate if
        rounding occurs.  The rounding mode is determined by the rounding parameter
        if given, else by the given context. If neither parameter is given, then the
        rounding mode of the current default context is used.

        """

    def to_integral_value(self, rounding: str | None = None, context: Context | None = None) -> Decimal:
        """Round to the nearest integer without signaling Inexact or Rounded.  The
        rounding mode is determined by the rounding parameter if given, else by
        the given context. If neither parameter is given, then the rounding mode
        of the current default context is used.

        """

    def to_integral(self, rounding: str | None = None, context: Context | None = None) -> Decimal:
        """Identical to the to_integral_value() method.  The to_integral() name has been
        kept for compatibility with older versions.

        """

    def sqrt(self, context: Context | None = None) -> Decimal:
        """Return the square root of the argument to full precision. The result is
        correctly rounded using the ROUND_HALF_EVEN rounding mode.

        """

    def max(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Maximum of self and other.  If one operand is a quiet NaN and the other is
        numeric, the numeric operand is returned.

        """

    def min(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Minimum of self and other. If one operand is a quiet NaN and the other is
        numeric, the numeric operand is returned.

        """

    def adjusted(self) -> int:
        """Return the adjusted exponent of the number.  Defined as exp + digits - 1."""

    def canonical(self) -> Decimal:
        """Return the canonical encoding of the argument.  Currently, the encoding
        of a Decimal instance is always canonical, so this operation returns its
        argument unchanged.

        """

    def compare_signal(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Identical to compare, except that all NaNs signal."""

    def compare_total(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Compare two operands using their abstract representation rather than
        their numerical value.  Similar to the compare() method, but the result
        gives a total ordering on Decimal instances.  Two Decimal instances with
        the same numeric value but different representations compare unequal
        in this ordering:

            >>> Decimal('12.0').compare_total(Decimal('12'))
            Decimal('-1')

        Quiet and signaling NaNs are also included in the total ordering. The result
        of this function is Decimal('0') if both operands have the same representation,
        Decimal('-1') if the first operand is lower in the total order than the second,
        and Decimal('1') if the first operand is higher in the total order than the
        second operand. See the specification for details of the total order.

        This operation is unaffected by context and is quiet: no flags are changed
        and no rounding is performed. As an exception, the C version may raise
        InvalidOperation if the second operand cannot be converted exactly.

        """

    def compare_total_mag(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Compare two operands using their abstract representation rather than their
        value as in compare_total(), but ignoring the sign of each operand.

        x.compare_total_mag(y) is equivalent to x.copy_abs().compare_total(y.copy_abs()).

        This operation is unaffected by context and is quiet: no flags are changed
        and no rounding is performed. As an exception, the C version may raise
        InvalidOperation if the second operand cannot be converted exactly.

        """

    def copy_abs(self) -> Decimal:
        """Return the absolute value of the argument.  This operation is unaffected by
        context and is quiet: no flags are changed and no rounding is performed.

        """

    def copy_negate(self) -> Decimal:
        """Return the negation of the argument.  This operation is unaffected by context
        and is quiet: no flags are changed and no rounding is performed.

        """

    def copy_sign(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Return a copy of the first operand with the sign set to be the same as the
        sign of the second operand. For example:

            >>> Decimal('2.3').copy_sign(Decimal('-1.5'))
            Decimal('-2.3')

        This operation is unaffected by context and is quiet: no flags are changed
        and no rounding is performed. As an exception, the C version may raise
        InvalidOperation if the second operand cannot be converted exactly.

        """

    def exp(self, context: Context | None = None) -> Decimal:
        """Return the value of the (natural) exponential function e**x at the given
        number.  The function always uses the ROUND_HALF_EVEN mode and the result
        is correctly rounded.

        """

    def is_canonical(self) -> bool:
        """Return True if the argument is canonical and False otherwise.  Currently,
        a Decimal instance is always canonical, so this operation always returns
        True.

        """

    def is_finite(self) -> bool:
        """Return True if the argument is a finite number, and False if the argument
        is infinite or a NaN.

        """

    def is_infinite(self) -> bool:
        """Return True if the argument is either positive or negative infinity and
        False otherwise.

        """

    def is_nan(self) -> bool:
        """Return True if the argument is a (quiet or signaling) NaN and False
        otherwise.

        """

    def is_normal(self, context: Context | None = None) -> bool:
        """Return True if the argument is a normal finite non-zero number with an
        adjusted exponent greater than or equal to Emin. Return False if the
        argument is zero, subnormal, infinite or a NaN.

        """

    def is_qnan(self) -> bool:
        """Return True if the argument is a quiet NaN, and False otherwise."""

    def is_signed(self) -> bool:
        """Return True if the argument has a negative sign and False otherwise.
        Note that both zeros and NaNs can carry signs.

        """

    def is_snan(self) -> bool:
        """Return True if the argument is a signaling NaN and False otherwise."""

    def is_subnormal(self, context: Context | None = None) -> bool:
        """Return True if the argument is subnormal, and False otherwise. A number is
        subnormal if it is non-zero, finite, and has an adjusted exponent less
        than Emin.

        """

    def is_zero(self) -> bool:
        """Return True if the argument is a (positive or negative) zero and False
        otherwise.

        """

    def ln(self, context: Context | None = None) -> Decimal:
        """Return the natural (base e) logarithm of the operand. The function always
        uses the ROUND_HALF_EVEN mode and the result is correctly rounded.

        """

    def log10(self, context: Context | None = None) -> Decimal:
        """Return the base ten logarithm of the operand. The function always uses the
        ROUND_HALF_EVEN mode and the result is correctly rounded.

        """

    def logb(self, context: Context | None = None) -> Decimal:
        """For a non-zero number, return the adjusted exponent of the operand as a
        Decimal instance.  If the operand is a zero, then Decimal('-Infinity') is
        returned and the DivisionByZero condition is raised. If the operand is
        an infinity then Decimal('Infinity') is returned.

        """

    def logical_and(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Applies an 'and' operation between self and other's digits.

        Both self and other must be logical numbers.

        """

    def logical_invert(self, context: Context | None = None) -> Decimal:
        """Invert all its digits.

        The self must be logical number.

        """

    def logical_or(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Applies an 'or' operation between self and other's digits.

        Both self and other must be logical numbers.

        """

    def logical_xor(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Applies an 'xor' operation between self and other's digits.

        Both self and other must be logical numbers.

        """

    def max_mag(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Similar to the max() method, but the comparison is done using the absolute
        values of the operands.

        """

    def min_mag(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Similar to the min() method, but the comparison is done using the absolute
        values of the operands.

        """

    def next_minus(self, context: Context | None = None) -> Decimal:
        """Return the largest number representable in the given context (or in the
        current default context if no context is given) that is smaller than the
        given operand.

        """

    def next_plus(self, context: Context | None = None) -> Decimal:
        """Return the smallest number representable in the given context (or in the
        current default context if no context is given) that is larger than the
        given operand.

        """

    def next_toward(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """If the two operands are unequal, return the number closest to the first
        operand in the direction of the second operand.  If both operands are
        numerically equal, return a copy of the first operand with the sign set
        to be the same as the sign of the second operand.

        """

    def number_class(self, context: Context | None = None) -> str:
        """Return a string describing the class of the operand.  The returned value
        is one of the following ten strings:

            * '-Infinity', indicating that the operand is negative infinity.
            * '-Normal', indicating that the operand is a negative normal number.
            * '-Subnormal', indicating that the operand is negative and subnormal.
            * '-Zero', indicating that the operand is a negative zero.
            * '+Zero', indicating that the operand is a positive zero.
            * '+Subnormal', indicating that the operand is positive and subnormal.
            * '+Normal', indicating that the operand is a positive normal number.
            * '+Infinity', indicating that the operand is positive infinity.
            * 'NaN', indicating that the operand is a quiet NaN (Not a Number).
            * 'sNaN', indicating that the operand is a signaling NaN.


        """

    def radix(self) -> Decimal:
        """Return Decimal(10), the radix (base) in which the Decimal class does
        all its arithmetic. Included for compatibility with the specification.

        """

    def rotate(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Return the result of rotating the digits of the first operand by an amount
        specified by the second operand.  The second operand must be an integer in
        the range -precision through precision. The absolute value of the second
        operand gives the number of places to rotate. If the second operand is
        positive then rotation is to the left; otherwise rotation is to the right.
        The coefficient of the first operand is padded on the left with zeros to
        length precision if necessary. The sign and exponent of the first operand are
        unchanged.

        """

    def scaleb(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Return the first operand with the exponent adjusted the second.  Equivalently,
        return the first operand multiplied by 10**other. The second operand must be
        an integer.

        """

    def shift(self, other: _Decimal, context: Context | None = None) -> Decimal:
        """Return the result of shifting the digits of the first operand by an amount
        specified by the second operand.  The second operand must be an integer in
        the range -precision through precision. The absolute value of the second
        operand gives the number of places to shift. If the second operand is
        positive, then the shift is to the left; otherwise the shift is to the
        right. Digits shifted into the coefficient are zeros. The sign and exponent
        of the first operand are unchanged.

        """

    def __reduce__(self) -> tuple[type[Self], tuple[str]]: ...
    def __copy__(self) -> Self: ...
    def __deepcopy__(self, memo: Any, /) -> Self: ...
    def __format__(self, specifier: str, context: Context | None = None, /) -> str: ...

@disjoint_base
class Context:
    """The context affects almost all operations and controls rounding,
    Over/Underflow, raising of exceptions and much more.  A new context
    can be constructed as follows:

        >>> c = Context(prec=28, Emin=-425000000, Emax=425000000,
        ...             rounding=ROUND_HALF_EVEN, capitals=1, clamp=1,
        ...             traps=[InvalidOperation, DivisionByZero, Overflow],
        ...             flags=[])
        >>>


    """

    # TODO: Context doesn't allow you to delete *any* attributes from instances of the class at runtime,
    # even settable attributes like `prec` and `rounding`,
    # but that's inexpressible in the stub.
    # Type checkers either ignore it or misinterpret it
    # if you add a `def __delattr__(self, name: str, /) -> NoReturn` method to the stub
    prec: int
    rounding: str
    Emin: int
    Emax: int
    capitals: int
    clamp: int
    traps: dict[_TrapType, bool]
    flags: dict[_TrapType, bool]
    def __init__(
        self,
        prec: int | None = None,
        rounding: str | None = None,
        Emin: int | None = None,
        Emax: int | None = None,
        capitals: int | None = None,
        clamp: int | None = None,
        flags: dict[_TrapType, bool] | Container[_TrapType] | None = None,
        traps: dict[_TrapType, bool] | Container[_TrapType] | None = None,
    ) -> None: ...
    def __reduce__(self) -> tuple[type[Self], tuple[Any, ...]]: ...
    def clear_flags(self) -> None:
        """Reset all flags to False."""

    def clear_traps(self) -> None:
        """Set all traps to False."""

    def copy(self) -> Context:
        """Return a duplicate of the context with all flags cleared."""

    def __copy__(self) -> Context: ...
    # see https://github.com/python/cpython/issues/94107
    __hash__: ClassVar[None]  # type: ignore[assignment]
    def Etiny(self) -> int:
        """Return a value equal to Emin - prec + 1, which is the minimum exponent value
        for subnormal results.  When underflow occurs, the exponent is set to Etiny.

        """

    def Etop(self) -> int:
        """Return a value equal to Emax - prec + 1.  This is the maximum exponent
        if the _clamp field of the context is set to 1 (IEEE clamp mode).  Etop()
        must not be negative.

        """

    def create_decimal(self, num: _DecimalNew = "0", /) -> Decimal:
        """Create a new Decimal instance from num, using self as the context. Unlike the
        Decimal constructor, this function observes the context limits.

        """

    def create_decimal_from_float(self, f: float, /) -> Decimal:
        """Create a new Decimal instance from float f.  Unlike the Decimal.from_float()
        class method, this function observes the context limits.

        """

    def abs(self, x: _Decimal, /) -> Decimal:
        """Return the absolute value of x."""

    def add(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return the sum of x and y."""

    def canonical(self, x: Decimal, /) -> Decimal:
        """Return a new instance of x."""

    def compare(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Compare x and y numerically."""

    def compare_signal(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Compare x and y numerically.  All NaNs signal."""

    def compare_total(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Compare x and y using their abstract representation."""

    def compare_total_mag(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Compare x and y using their abstract representation, ignoring sign."""

    def copy_abs(self, x: _Decimal, /) -> Decimal:
        """Return a copy of x with the sign set to 0."""

    def copy_decimal(self, x: _Decimal, /) -> Decimal:
        """Return a copy of Decimal x."""

    def copy_negate(self, x: _Decimal, /) -> Decimal:
        """Return a copy of x with the sign inverted."""

    def copy_sign(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Copy the sign from y to x."""

    def divide(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return x divided by y."""

    def divide_int(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return x divided by y, truncated to an integer."""

    def divmod(self, x: _Decimal, y: _Decimal, /) -> tuple[Decimal, Decimal]:
        """Return quotient and remainder of the division x / y."""

    def exp(self, x: _Decimal, /) -> Decimal:
        """Return e ** x."""

    def fma(self, x: _Decimal, y: _Decimal, z: _Decimal, /) -> Decimal:
        """Return x multiplied by y, plus z."""

    def is_canonical(self, x: _Decimal, /) -> bool:
        """Return True if x is canonical, False otherwise."""

    def is_finite(self, x: _Decimal, /) -> bool:
        """Return True if x is finite, False otherwise."""

    def is_infinite(self, x: _Decimal, /) -> bool:
        """Return True if x is infinite, False otherwise."""

    def is_nan(self, x: _Decimal, /) -> bool:
        """Return True if x is a qNaN or sNaN, False otherwise."""

    def is_normal(self, x: _Decimal, /) -> bool:
        """Return True if x is a normal number, False otherwise."""

    def is_qnan(self, x: _Decimal, /) -> bool:
        """Return True if x is a quiet NaN, False otherwise."""

    def is_signed(self, x: _Decimal, /) -> bool:
        """Return True if x is negative, False otherwise."""

    def is_snan(self, x: _Decimal, /) -> bool:
        """Return True if x is a signaling NaN, False otherwise."""

    def is_subnormal(self, x: _Decimal, /) -> bool:
        """Return True if x is subnormal, False otherwise."""

    def is_zero(self, x: _Decimal, /) -> bool:
        """Return True if x is a zero, False otherwise."""

    def ln(self, x: _Decimal, /) -> Decimal:
        """Return the natural (base e) logarithm of x."""

    def log10(self, x: _Decimal, /) -> Decimal:
        """Return the base 10 logarithm of x."""

    def logb(self, x: _Decimal, /) -> Decimal:
        """Return the exponent of the magnitude of the operand's MSD."""

    def logical_and(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Applies the logical operation 'and' between each operand's digits.

        The operands must be both logical numbers.

            >>> ExtendedContext.logical_and(Decimal('0'), Decimal('0'))
            Decimal('0')
            >>> ExtendedContext.logical_and(Decimal('0'), Decimal('1'))
            Decimal('0')
            >>> ExtendedContext.logical_and(Decimal('1'), Decimal('0'))
            Decimal('0')
            >>> ExtendedContext.logical_and(Decimal('1'), Decimal('1'))
            Decimal('1')
            >>> ExtendedContext.logical_and(Decimal('1100'), Decimal('1010'))
            Decimal('1000')
            >>> ExtendedContext.logical_and(Decimal('1111'), Decimal('10'))
            Decimal('10')
            >>> ExtendedContext.logical_and(110, 1101)
            Decimal('100')
            >>> ExtendedContext.logical_and(Decimal(110), 1101)
            Decimal('100')
            >>> ExtendedContext.logical_and(110, Decimal(1101))
            Decimal('100')

        """

    def logical_invert(self, x: _Decimal, /) -> Decimal:
        """Invert all the digits in the operand.

        The operand must be a logical number.

            >>> ExtendedContext.logical_invert(Decimal('0'))
            Decimal('111111111')
            >>> ExtendedContext.logical_invert(Decimal('1'))
            Decimal('111111110')
            >>> ExtendedContext.logical_invert(Decimal('111111111'))
            Decimal('0')
            >>> ExtendedContext.logical_invert(Decimal('101010101'))
            Decimal('10101010')
            >>> ExtendedContext.logical_invert(1101)
            Decimal('111110010')

        """

    def logical_or(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Applies the logical operation 'or' between each operand's digits.

        The operands must be both logical numbers.

            >>> ExtendedContext.logical_or(Decimal('0'), Decimal('0'))
            Decimal('0')
            >>> ExtendedContext.logical_or(Decimal('0'), Decimal('1'))
            Decimal('1')
            >>> ExtendedContext.logical_or(Decimal('1'), Decimal('0'))
            Decimal('1')
            >>> ExtendedContext.logical_or(Decimal('1'), Decimal('1'))
            Decimal('1')
            >>> ExtendedContext.logical_or(Decimal('1100'), Decimal('1010'))
            Decimal('1110')
            >>> ExtendedContext.logical_or(Decimal('1110'), Decimal('10'))
            Decimal('1110')
            >>> ExtendedContext.logical_or(110, 1101)
            Decimal('1111')
            >>> ExtendedContext.logical_or(Decimal(110), 1101)
            Decimal('1111')
            >>> ExtendedContext.logical_or(110, Decimal(1101))
            Decimal('1111')

        """

    def logical_xor(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Applies the logical operation 'xor' between each operand's digits.

        The operands must be both logical numbers.

            >>> ExtendedContext.logical_xor(Decimal('0'), Decimal('0'))
            Decimal('0')
            >>> ExtendedContext.logical_xor(Decimal('0'), Decimal('1'))
            Decimal('1')
            >>> ExtendedContext.logical_xor(Decimal('1'), Decimal('0'))
            Decimal('1')
            >>> ExtendedContext.logical_xor(Decimal('1'), Decimal('1'))
            Decimal('0')
            >>> ExtendedContext.logical_xor(Decimal('1100'), Decimal('1010'))
            Decimal('110')
            >>> ExtendedContext.logical_xor(Decimal('1111'), Decimal('10'))
            Decimal('1101')
            >>> ExtendedContext.logical_xor(110, 1101)
            Decimal('1011')
            >>> ExtendedContext.logical_xor(Decimal(110), 1101)
            Decimal('1011')
            >>> ExtendedContext.logical_xor(110, Decimal(1101))
            Decimal('1011')

        """

    def max(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Compare the values numerically and return the maximum."""

    def max_mag(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Compare the values numerically with their sign ignored."""

    def min(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Compare the values numerically and return the minimum."""

    def min_mag(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Compare the values numerically with their sign ignored."""

    def minus(self, x: _Decimal, /) -> Decimal:
        """Minus corresponds to the unary prefix minus operator in Python, but applies
        the context to the result.

        """

    def multiply(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return the product of x and y."""

    def next_minus(self, x: _Decimal, /) -> Decimal:
        """Return the largest representable number smaller than x."""

    def next_plus(self, x: _Decimal, /) -> Decimal:
        """Return the smallest representable number larger than x."""

    def next_toward(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return the number closest to x, in the direction towards y."""

    def normalize(self, x: _Decimal, /) -> Decimal:
        """Reduce x to its simplest form. Alias for reduce(x)."""

    def number_class(self, x: _Decimal, /) -> str:
        """Return an indication of the class of x."""

    def plus(self, x: _Decimal, /) -> Decimal:
        """Plus corresponds to the unary prefix plus operator in Python, but applies
        the context to the result.

        """

    def power(self, a: _Decimal, b: _Decimal, modulo: _Decimal | None = None) -> Decimal:
        """Compute a**b. If 'a' is negative, then 'b' must be integral. The result
        will be inexact unless 'a' is integral and the result is finite and can
        be expressed exactly in 'precision' digits.  In the Python version the
        result is always correctly rounded, in the C version the result is almost
        always correctly rounded.

        If modulo is given, compute (a**b) % modulo. The following restrictions
        hold:

            * all three arguments must be integral
            * 'b' must be nonnegative
            * at least one of 'a' or 'b' must be nonzero
            * modulo must be nonzero and less than 10**prec in absolute value


        """

    def quantize(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return a value equal to x (rounded), having the exponent of y."""

    def radix(self) -> Decimal:
        """Return 10."""

    def remainder(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return the remainder from integer division.  The sign of the result,
        if non-zero, is the same as that of the original dividend.

        """

    def remainder_near(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return x - y * n, where n is the integer nearest the exact value of x / y
        (if the result is 0 then its sign will be the sign of x).

        """

    def rotate(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return a copy of x, rotated by y places."""

    def same_quantum(self, x: _Decimal, y: _Decimal, /) -> bool:
        """Return True if the two operands have the same exponent."""

    def scaleb(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return the first operand after adding the second value to its exp."""

    def shift(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return a copy of x, shifted by y places."""

    def sqrt(self, x: _Decimal, /) -> Decimal:
        """Square root of a non-negative number to context precision."""

    def subtract(self, x: _Decimal, y: _Decimal, /) -> Decimal:
        """Return the difference between x and y."""

    def to_eng_string(self, x: _Decimal, /) -> str:
        """Convert a number to a string, using engineering notation."""

    def to_sci_string(self, x: _Decimal, /) -> str:
        """Convert a number to a string using scientific notation."""

    def to_integral_exact(self, x: _Decimal, /) -> Decimal:
        """Round to an integer. Signal if the result is rounded or inexact."""

    def to_integral_value(self, x: _Decimal, /) -> Decimal:
        """Round to an integer."""

    def to_integral(self, x: _Decimal, /) -> Decimal:
        """Identical to to_integral_value(x)."""
