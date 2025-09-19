"""Fraction, infinite-precision, rational numbers."""

import sys
from collections.abc import Callable
from decimal import Decimal
from numbers import Rational, Real
from typing import Any, Literal, Protocol, SupportsIndex, overload, type_check_only
from typing_extensions import Self, TypeAlias

_ComparableNum: TypeAlias = int | float | Decimal | Real

__all__ = ["Fraction"]

@type_check_only
class _ConvertibleToIntegerRatio(Protocol):
    def as_integer_ratio(self) -> tuple[int | Rational, int | Rational]: ...

class Fraction(Rational):
    """This class implements rational numbers.

    In the two-argument form of the constructor, Fraction(8, 6) will
    produce a rational number equivalent to 4/3. Both arguments must
    be Rational. The numerator defaults to 0 and the denominator
    defaults to 1 so that Fraction(3) == 3 and Fraction() == 0.

    Fractions can also be constructed from:

      - numeric strings similar to those accepted by the
        float constructor (for example, '-2.3' or '1e10')

      - strings of the form '123/456'

      - float and Decimal instances

      - other Rational instances (including integers)

    """

    __slots__ = ("_numerator", "_denominator")
    @overload
    def __new__(cls, numerator: int | Rational = 0, denominator: int | Rational | None = None) -> Self:
        """Constructs a Rational.

        Takes a string like '3/2' or '1.5', another Rational instance, a
        numerator/denominator pair, or a float.

        Examples
        --------

        >>> Fraction(10, -8)
        Fraction(-5, 4)
        >>> Fraction(Fraction(1, 7), 5)
        Fraction(1, 35)
        >>> Fraction(Fraction(1, 7), Fraction(2, 3))
        Fraction(3, 14)
        >>> Fraction('314')
        Fraction(314, 1)
        >>> Fraction('-35/4')
        Fraction(-35, 4)
        >>> Fraction('3.1415') # conversion from numeric string
        Fraction(6283, 2000)
        >>> Fraction('-47e-2') # string may include a decimal exponent
        Fraction(-47, 100)
        >>> Fraction(1.47)  # direct construction from float (exact conversion)
        Fraction(6620291452234629, 4503599627370496)
        >>> Fraction(2.25)
        Fraction(9, 4)
        >>> Fraction(Decimal('1.47'))
        Fraction(147, 100)

        """

    @overload
    def __new__(cls, numerator: float | Decimal | str) -> Self: ...

    if sys.version_info >= (3, 14):
        @overload
        def __new__(cls, numerator: _ConvertibleToIntegerRatio) -> Self:
            """Constructs a Rational.

            Takes a string like '3/2' or '1.5', another Rational instance, a
            numerator/denominator pair, or a float.

            Examples
            --------

            >>> Fraction(10, -8)
            Fraction(-5, 4)
            >>> Fraction(Fraction(1, 7), 5)
            Fraction(1, 35)
            >>> Fraction(Fraction(1, 7), Fraction(2, 3))
            Fraction(3, 14)
            >>> Fraction('314')
            Fraction(314, 1)
            >>> Fraction('-35/4')
            Fraction(-35, 4)
            >>> Fraction('3.1415') # conversion from numeric string
            Fraction(6283, 2000)
            >>> Fraction('-47e-2') # string may include a decimal exponent
            Fraction(-47, 100)
            >>> Fraction(1.47)  # direct construction from float (exact conversion)
            Fraction(6620291452234629, 4503599627370496)
            >>> Fraction(2.25)
            Fraction(9, 4)
            >>> Fraction(Decimal('1.47'))
            Fraction(147, 100)

            """

    @classmethod
    def from_float(cls, f: float) -> Self:
        """Converts a finite float to a rational number, exactly.

        Beware that Fraction.from_float(0.3) != Fraction(3, 10).

        """

    @classmethod
    def from_decimal(cls, dec: Decimal) -> Self:
        """Converts a finite Decimal instance to a rational number, exactly."""

    def limit_denominator(self, max_denominator: int = 1000000) -> Fraction:
        """Closest Fraction to self with denominator at most max_denominator.

        >>> Fraction('3.141592653589793').limit_denominator(10)
        Fraction(22, 7)
        >>> Fraction('3.141592653589793').limit_denominator(100)
        Fraction(311, 99)
        >>> Fraction(4321, 8765).limit_denominator(10000)
        Fraction(4321, 8765)

        """

    def as_integer_ratio(self) -> tuple[int, int]:
        """Return a pair of integers, whose ratio is equal to the original Fraction.

        The ratio is in lowest terms and has a positive denominator.
        """
    if sys.version_info >= (3, 12):
        def is_integer(self) -> bool:
            """Return True if the Fraction is an integer."""

    @property
    def numerator(a) -> int: ...
    @property
    def denominator(a) -> int: ...
    @overload
    def __add__(a, b: int | Fraction) -> Fraction:
        """a + b"""

    @overload
    def __add__(a, b: float) -> float: ...
    @overload
    def __add__(a, b: complex) -> complex: ...
    @overload
    def __radd__(b, a: int | Fraction) -> Fraction:
        """a + b"""

    @overload
    def __radd__(b, a: float) -> float: ...
    @overload
    def __radd__(b, a: complex) -> complex: ...
    @overload
    def __sub__(a, b: int | Fraction) -> Fraction:
        """a - b"""

    @overload
    def __sub__(a, b: float) -> float: ...
    @overload
    def __sub__(a, b: complex) -> complex: ...
    @overload
    def __rsub__(b, a: int | Fraction) -> Fraction:
        """a - b"""

    @overload
    def __rsub__(b, a: float) -> float: ...
    @overload
    def __rsub__(b, a: complex) -> complex: ...
    @overload
    def __mul__(a, b: int | Fraction) -> Fraction:
        """a * b"""

    @overload
    def __mul__(a, b: float) -> float: ...
    @overload
    def __mul__(a, b: complex) -> complex: ...
    @overload
    def __rmul__(b, a: int | Fraction) -> Fraction:
        """a * b"""

    @overload
    def __rmul__(b, a: float) -> float: ...
    @overload
    def __rmul__(b, a: complex) -> complex: ...
    @overload
    def __truediv__(a, b: int | Fraction) -> Fraction:
        """a / b"""

    @overload
    def __truediv__(a, b: float) -> float: ...
    @overload
    def __truediv__(a, b: complex) -> complex: ...
    @overload
    def __rtruediv__(b, a: int | Fraction) -> Fraction:
        """a / b"""

    @overload
    def __rtruediv__(b, a: float) -> float: ...
    @overload
    def __rtruediv__(b, a: complex) -> complex: ...
    @overload
    def __floordiv__(a, b: int | Fraction) -> int:
        """a // b"""

    @overload
    def __floordiv__(a, b: float) -> float: ...
    @overload
    def __rfloordiv__(b, a: int | Fraction) -> int:
        """a // b"""

    @overload
    def __rfloordiv__(b, a: float) -> float: ...
    @overload
    def __mod__(a, b: int | Fraction) -> Fraction:
        """a % b"""

    @overload
    def __mod__(a, b: float) -> float: ...
    @overload
    def __rmod__(b, a: int | Fraction) -> Fraction:
        """a % b"""

    @overload
    def __rmod__(b, a: float) -> float: ...
    @overload
    def __divmod__(a, b: int | Fraction) -> tuple[int, Fraction]:
        """(a // b, a % b)"""

    @overload
    def __divmod__(a, b: float) -> tuple[float, Fraction]: ...
    @overload
    def __rdivmod__(a, b: int | Fraction) -> tuple[int, Fraction]:
        """(a // b, a % b)"""

    @overload
    def __rdivmod__(a, b: float) -> tuple[float, Fraction]: ...
    if sys.version_info >= (3, 14):
        @overload
        def __pow__(a, b: int, modulo: None = None) -> Fraction:
            """a ** b

            If b is not an integer, the result will be a float or complex
            since roots are generally irrational. If b is an integer, the
            result will be rational.

            """

        @overload
        def __pow__(a, b: float | Fraction, modulo: None = None) -> float: ...
        @overload
        def __pow__(a, b: complex, modulo: None = None) -> complex: ...
    else:
        @overload
        def __pow__(a, b: int) -> Fraction:
            """a ** b

            If b is not an integer, the result will be a float or complex
            since roots are generally irrational. If b is an integer, the
            result will be rational.

            """

        @overload
        def __pow__(a, b: float | Fraction) -> float: ...
        @overload
        def __pow__(a, b: complex) -> complex: ...
    if sys.version_info >= (3, 14):
        @overload
        def __rpow__(b, a: float | Fraction, modulo: None = None) -> float:
            """a ** b"""

        @overload
        def __rpow__(b, a: complex, modulo: None = None) -> complex: ...
    else:
        @overload
        def __rpow__(b, a: float | Fraction) -> float:
            """a ** b"""

        @overload
        def __rpow__(b, a: complex) -> complex: ...

    def __pos__(a) -> Fraction:
        """+a: Coerces a subclass instance to Fraction"""

    def __neg__(a) -> Fraction:
        """-a"""

    def __abs__(a) -> Fraction:
        """abs(a)"""

    def __trunc__(a) -> int:
        """math.trunc(a)"""

    def __floor__(a) -> int:
        """math.floor(a)"""

    def __ceil__(a) -> int:
        """math.ceil(a)"""

    @overload
    def __round__(self, ndigits: None = None) -> int:
        """round(self, ndigits)

        Rounds half toward even.
        """

    @overload
    def __round__(self, ndigits: int) -> Fraction: ...
    def __hash__(self) -> int:  # type: ignore[override]
        """hash(self)"""

    def __eq__(a, b: object) -> bool:
        """a == b"""

    def __lt__(a, b: _ComparableNum) -> bool:
        """a < b"""

    def __gt__(a, b: _ComparableNum) -> bool:
        """a > b"""

    def __le__(a, b: _ComparableNum) -> bool:
        """a <= b"""

    def __ge__(a, b: _ComparableNum) -> bool:
        """a >= b"""

    def __bool__(a) -> bool:
        """a != 0"""

    def __copy__(self) -> Self: ...
    def __deepcopy__(self, memo: Any) -> Self: ...
    if sys.version_info >= (3, 11):
        def __int__(a, _index: Callable[[SupportsIndex], int] = ...) -> int:
            """int(a)"""
    # Not actually defined within fractions.py, but provides more useful
    # overrides
    @property
    def real(self) -> Fraction:
        """Real numbers are their real component."""

    @property
    def imag(self) -> Literal[0]:
        """Real numbers have no imaginary component."""

    def conjugate(self) -> Fraction:
        """Conjugate is a no-op for Reals."""
    if sys.version_info >= (3, 14):
        @classmethod
        def from_number(cls, number: float | Rational | _ConvertibleToIntegerRatio) -> Self:
            """Converts a finite real number to a rational number, exactly.

            Beware that Fraction.from_number(0.3) != Fraction(3, 10).

            """
