"""This module provides access to integer related mathematical functions."""

from typing import SupportsIndex

def comb(n: SupportsIndex, k: SupportsIndex, /) -> int:
    """Number of ways to choose k items from n items without repetition and without order.

    Evaluates to n! / (k! * (n - k)!) when k <= n and evaluates
    to zero when k > n.

    Also called the binomial coefficient because it is equivalent
    to the coefficient of k-th term in polynomial expansion of the
    expression (1 + x)**n.

    Raises ValueError if either of the arguments are negative.
    """

def factorial(n: SupportsIndex, /) -> int:
    """Find n!."""

def gcd(*integers: SupportsIndex) -> int:
    """Greatest Common Divisor."""

def isqrt(n: SupportsIndex, /) -> int:
    """Return the integer part of the square root of the input."""

def lcm(*integers: SupportsIndex) -> int:
    """Least Common Multiple."""

def perm(n: SupportsIndex, k: SupportsIndex | None = None, /) -> int:
    """Number of ways to choose k items from n items without repetition and with order.

    Evaluates to n! / (n - k)! when k <= n and evaluates
    to zero when k > n.

    If k is not specified or is None, then k defaults to n
    and the function returns n!.

    Raises ValueError if either of the arguments are negative.
    """
