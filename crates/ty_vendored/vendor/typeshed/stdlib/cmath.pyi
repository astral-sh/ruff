"""This module provides access to mathematical functions for complex
numbers.
"""

from typing import Final, SupportsComplex, SupportsFloat, SupportsIndex
from typing_extensions import TypeAlias

e: Final[float]
pi: Final[float]
inf: Final[float]
infj: Final[complex]
nan: Final[float]
nanj: Final[complex]
tau: Final[float]

_C: TypeAlias = SupportsFloat | SupportsComplex | SupportsIndex | complex

def acos(z: _C, /) -> complex:
    """Return the arc cosine of z."""

def acosh(z: _C, /) -> complex:
    """Return the inverse hyperbolic cosine of z."""

def asin(z: _C, /) -> complex:
    """Return the arc sine of z."""

def asinh(z: _C, /) -> complex:
    """Return the inverse hyperbolic sine of z."""

def atan(z: _C, /) -> complex:
    """Return the arc tangent of z."""

def atanh(z: _C, /) -> complex:
    """Return the inverse hyperbolic tangent of z."""

def cos(z: _C, /) -> complex:
    """Return the cosine of z."""

def cosh(z: _C, /) -> complex:
    """Return the hyperbolic cosine of z."""

def exp(z: _C, /) -> complex:
    """Return the exponential value e**z."""

def isclose(a: _C, b: _C, *, rel_tol: SupportsFloat = 1e-09, abs_tol: SupportsFloat = 0.0) -> bool:
    """Determine whether two complex numbers are close in value.

      rel_tol
        maximum difference for being considered "close", relative to the
        magnitude of the input values
      abs_tol
        maximum difference for being considered "close", regardless of the
        magnitude of the input values

    Return True if a is close in value to b, and False otherwise.

    For the values to be considered close, the difference between them must be
    smaller than at least one of the tolerances.

    -inf, inf and NaN behave similarly to the IEEE 754 Standard. That is, NaN is
    not close to anything, even itself. inf and -inf are only close to themselves.
    """

def isinf(z: _C, /) -> bool:
    """Checks if the real or imaginary part of z is infinite."""

def isnan(z: _C, /) -> bool:
    """Checks if the real or imaginary part of z not a number (NaN)."""

def log(z: _C, base: _C = ..., /) -> complex:
    """log(z[, base]) -> the logarithm of z to the given base.

    If the base is not specified, returns the natural logarithm (base e) of z.
    """

def log10(z: _C, /) -> complex:
    """Return the base-10 logarithm of z."""

def phase(z: _C, /) -> float:
    """Return argument, also known as the phase angle, of a complex."""

def polar(z: _C, /) -> tuple[float, float]:
    """Convert a complex from rectangular coordinates to polar coordinates.

    r is the distance from 0 and phi the phase angle.
    """

def rect(r: float, phi: float, /) -> complex:
    """Convert from polar coordinates to rectangular coordinates."""

def sin(z: _C, /) -> complex:
    """Return the sine of z."""

def sinh(z: _C, /) -> complex:
    """Return the hyperbolic sine of z."""

def sqrt(z: _C, /) -> complex:
    """Return the square root of z."""

def tan(z: _C, /) -> complex:
    """Return the tangent of z."""

def tanh(z: _C, /) -> complex:
    """Return the hyperbolic tangent of z."""

def isfinite(z: _C, /) -> bool:
    """Return True if both the real and imaginary parts of z are finite, else False."""
