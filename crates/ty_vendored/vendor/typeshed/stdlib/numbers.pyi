"""Abstract Base Classes (ABCs) for numbers, according to PEP 3141.

TODO: Fill out more detailed documentation on the operators.
"""

# Note: these stubs are incomplete. The more complex type
# signatures are currently omitted.
#
# Use _ComplexLike, _RealLike and _IntegralLike for return types in this module
# rather than `numbers.Complex`, `numbers.Real` and `numbers.Integral`,
# to avoid an excessive number of `type: ignore`s in subclasses of these ABCs
# (since type checkers don't see `complex` as a subtype of `numbers.Complex`,
# nor `float` as a subtype of `numbers.Real`, etc.)

from abc import ABCMeta, abstractmethod
from typing import ClassVar, Literal, Protocol, overload, type_check_only

__all__ = ["Number", "Complex", "Real", "Rational", "Integral"]

############################
# Protocols for return types
############################

# `_ComplexLike` is a structural-typing approximation
# of the `Complex` ABC, which is not (and cannot be) a protocol
#
# NOTE: We can't include `__complex__` here,
# as we want `int` to be seen as a subtype of `_ComplexLike`,
# and `int.__complex__` does not exist :(
@type_check_only
class _ComplexLike(Protocol):
    def __neg__(self) -> _ComplexLike: ...
    def __pos__(self) -> _ComplexLike: ...
    def __abs__(self) -> _RealLike: ...

# _RealLike is a structural-typing approximation
# of the `Real` ABC, which is not (and cannot be) a protocol
@type_check_only
class _RealLike(_ComplexLike, Protocol):
    def __trunc__(self) -> _IntegralLike: ...
    def __floor__(self) -> _IntegralLike: ...
    def __ceil__(self) -> _IntegralLike: ...
    def __float__(self) -> float: ...
    # Overridden from `_ComplexLike`
    # for a more precise return type:
    def __neg__(self) -> _RealLike: ...
    def __pos__(self) -> _RealLike: ...

# _IntegralLike is a structural-typing approximation
# of the `Integral` ABC, which is not (and cannot be) a protocol
@type_check_only
class _IntegralLike(_RealLike, Protocol):
    def __invert__(self) -> _IntegralLike: ...
    def __int__(self) -> int: ...
    def __index__(self) -> int: ...
    # Overridden from `_ComplexLike`
    # for a more precise return type:
    def __abs__(self) -> _IntegralLike: ...
    # Overridden from `RealLike`
    # for a more precise return type:
    def __neg__(self) -> _IntegralLike: ...
    def __pos__(self) -> _IntegralLike: ...

#################
# Module "proper"
#################

class Number(metaclass=ABCMeta):
    """All numbers inherit from this class.

    If you just want to check if an argument x is a number, without
    caring what kind, use isinstance(x, Number).
    """

    __slots__ = ()
    @abstractmethod
    def __hash__(self) -> int:
        """The type of the None singleton."""

# See comment at the top of the file
# for why some of these return types are purposefully vague
class Complex(Number, _ComplexLike):
    """Complex defines the operations that work on the builtin complex type.

    In short, those are: a conversion to complex, .real, .imag, +, -,
    *, /, **, abs(), .conjugate, ==, and !=.

    If it is given heterogeneous arguments, and doesn't have special
    knowledge about them, it should fall back to the builtin complex
    type as described below.
    """

    __slots__ = ()
    @abstractmethod
    def __complex__(self) -> complex:
        """Return a builtin complex instance. Called for complex(self)."""

    def __bool__(self) -> bool:
        """True if self != 0. Called for bool(self)."""

    @property
    @abstractmethod
    def real(self) -> _RealLike:
        """Retrieve the real component of this number.

        This should subclass Real.
        """

    @property
    @abstractmethod
    def imag(self) -> _RealLike:
        """Retrieve the imaginary component of this number.

        This should subclass Real.
        """

    @abstractmethod
    def __add__(self, other) -> _ComplexLike:
        """self + other"""

    @abstractmethod
    def __radd__(self, other) -> _ComplexLike:
        """other + self"""

    @abstractmethod
    def __neg__(self) -> _ComplexLike:
        """-self"""

    @abstractmethod
    def __pos__(self) -> _ComplexLike:
        """+self"""

    def __sub__(self, other) -> _ComplexLike:
        """self - other"""

    def __rsub__(self, other) -> _ComplexLike:
        """other - self"""

    @abstractmethod
    def __mul__(self, other) -> _ComplexLike:
        """self * other"""

    @abstractmethod
    def __rmul__(self, other) -> _ComplexLike:
        """other * self"""

    @abstractmethod
    def __truediv__(self, other) -> _ComplexLike:
        """self / other: Should promote to float when necessary."""

    @abstractmethod
    def __rtruediv__(self, other) -> _ComplexLike:
        """other / self"""

    @abstractmethod
    def __pow__(self, exponent) -> _ComplexLike:
        """self ** exponent; should promote to float or complex when necessary."""

    @abstractmethod
    def __rpow__(self, base) -> _ComplexLike:
        """base ** self"""

    @abstractmethod
    def __abs__(self) -> _RealLike:
        """Returns the Real distance from 0. Called for abs(self)."""

    @abstractmethod
    def conjugate(self) -> _ComplexLike:
        """(x+y*i).conjugate() returns (x-y*i)."""

    @abstractmethod
    def __eq__(self, other: object) -> bool:
        """self == other"""
    __hash__: ClassVar[None]  # type: ignore[assignment]

# See comment at the top of the file
# for why some of these return types are purposefully vague
class Real(Complex, _RealLike):
    """To Complex, Real adds the operations that work on real numbers.

    In short, those are: a conversion to float, trunc(), divmod,
    %, <, <=, >, and >=.

    Real also provides defaults for the derived operations.
    """

    __slots__ = ()
    @abstractmethod
    def __float__(self) -> float:
        """Any Real can be converted to a native float object.

        Called for float(self).
        """

    @abstractmethod
    def __trunc__(self) -> _IntegralLike:
        """trunc(self): Truncates self to an Integral.

        Returns an Integral i such that:
          * i > 0 iff self > 0;
          * abs(i) <= abs(self);
          * for any Integral j satisfying the first two conditions,
            abs(i) >= abs(j) [i.e. i has "maximal" abs among those].
        i.e. "truncate towards 0".
        """

    @abstractmethod
    def __floor__(self) -> _IntegralLike:
        """Finds the greatest Integral <= self."""

    @abstractmethod
    def __ceil__(self) -> _IntegralLike:
        """Finds the least Integral >= self."""

    @abstractmethod
    @overload
    def __round__(self, ndigits: None = None) -> _IntegralLike:
        """Rounds self to ndigits decimal places, defaulting to 0.

        If ndigits is omitted or None, returns an Integral, otherwise
        returns a Real. Rounds half toward even.
        """

    @abstractmethod
    @overload
    def __round__(self, ndigits: int) -> _RealLike: ...
    def __divmod__(self, other) -> tuple[_RealLike, _RealLike]:
        """divmod(self, other): The pair (self // other, self % other).

        Sometimes this can be computed faster than the pair of
        operations.
        """

    def __rdivmod__(self, other) -> tuple[_RealLike, _RealLike]:
        """divmod(other, self): The pair (other // self, other % self).

        Sometimes this can be computed faster than the pair of
        operations.
        """

    @abstractmethod
    def __floordiv__(self, other) -> _RealLike:
        """self // other: The floor() of self/other."""

    @abstractmethod
    def __rfloordiv__(self, other) -> _RealLike:
        """other // self: The floor() of other/self."""

    @abstractmethod
    def __mod__(self, other) -> _RealLike:
        """self % other"""

    @abstractmethod
    def __rmod__(self, other) -> _RealLike:
        """other % self"""

    @abstractmethod
    def __lt__(self, other) -> bool:
        """self < other

        < on Reals defines a total ordering, except perhaps for NaN.
        """

    @abstractmethod
    def __le__(self, other) -> bool:
        """self <= other"""

    def __complex__(self) -> complex:
        """complex(self) == complex(float(self), 0)"""

    @property
    def real(self) -> _RealLike:
        """Real numbers are their real component."""

    @property
    def imag(self) -> Literal[0]:
        """Real numbers have no imaginary component."""

    def conjugate(self) -> _RealLike:
        """Conjugate is a no-op for Reals."""
    # Not actually overridden at runtime,
    # but we override these in the stub to give them more precise return types:
    @abstractmethod
    def __pos__(self) -> _RealLike:
        """+self"""

    @abstractmethod
    def __neg__(self) -> _RealLike:
        """-self"""

# See comment at the top of the file
# for why some of these return types are purposefully vague
class Rational(Real):
    """To Real, Rational adds numerator and denominator properties.

    The numerator and denominator values should be in lowest terms,
    with a positive denominator.
    """

    __slots__ = ()
    @property
    @abstractmethod
    def numerator(self) -> _IntegralLike:
        """The numerator of a rational number in lowest terms."""

    @property
    @abstractmethod
    def denominator(self) -> _IntegralLike:
        """The denominator of a rational number in lowest terms.

        This denominator should be positive.
        """

    def __float__(self) -> float:
        """float(self) = self.numerator / self.denominator

        It's important that this conversion use the integer's "true"
        division rather than casting one side to float before dividing
        so that ratios of huge integers convert without overflowing.

        """

# See comment at the top of the file
# for why some of these return types are purposefully vague
class Integral(Rational, _IntegralLike):
    """Integral adds methods that work on integral numbers.

    In short, these are conversion to int, pow with modulus, and the
    bit-string operations.
    """

    __slots__ = ()
    @abstractmethod
    def __int__(self) -> int:
        """int(self)"""

    def __index__(self) -> int:
        """Called whenever an index is needed, such as in slicing"""

    @abstractmethod
    def __pow__(self, exponent, modulus=None) -> _IntegralLike:
        """self ** exponent % modulus, but maybe faster.

        Accept the modulus argument if you want to support the
        3-argument version of pow(). Raise a TypeError if exponent < 0
        or any argument isn't Integral. Otherwise, just implement the
        2-argument version described in Complex.
        """

    @abstractmethod
    def __lshift__(self, other) -> _IntegralLike:
        """self << other"""

    @abstractmethod
    def __rlshift__(self, other) -> _IntegralLike:
        """other << self"""

    @abstractmethod
    def __rshift__(self, other) -> _IntegralLike:
        """self >> other"""

    @abstractmethod
    def __rrshift__(self, other) -> _IntegralLike:
        """other >> self"""

    @abstractmethod
    def __and__(self, other) -> _IntegralLike:
        """self & other"""

    @abstractmethod
    def __rand__(self, other) -> _IntegralLike:
        """other & self"""

    @abstractmethod
    def __xor__(self, other) -> _IntegralLike:
        """self ^ other"""

    @abstractmethod
    def __rxor__(self, other) -> _IntegralLike:
        """other ^ self"""

    @abstractmethod
    def __or__(self, other) -> _IntegralLike:
        """self | other"""

    @abstractmethod
    def __ror__(self, other) -> _IntegralLike:
        """other | self"""

    @abstractmethod
    def __invert__(self) -> _IntegralLike:
        """~self"""

    def __float__(self) -> float:
        """float(self) == float(int(self))"""

    @property
    def numerator(self) -> _IntegralLike:
        """Integers are their own numerators."""

    @property
    def denominator(self) -> Literal[1]:
        """Integers have a denominator of 1."""
    # Not actually overridden at runtime,
    # but we override these in the stub to give them more precise return types:
    @abstractmethod
    def __pos__(self) -> _IntegralLike:
        """+self"""

    @abstractmethod
    def __neg__(self) -> _IntegralLike:
        """-self"""

    @abstractmethod
    def __abs__(self) -> _IntegralLike:
        """Returns the Real distance from 0. Called for abs(self)."""

    @abstractmethod
    @overload
    def __round__(self, ndigits: None = None) -> _IntegralLike:
        """Rounds self to ndigits decimal places, defaulting to 0.

        If ndigits is omitted or None, returns an Integral, otherwise
        returns a Real. Rounds half toward even.
        """

    @abstractmethod
    @overload
    def __round__(self, ndigits: int) -> _IntegralLike: ...
